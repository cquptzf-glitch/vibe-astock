use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use calamine::{open_workbook_auto_from_rs, Reader};
use serde_json::{json, Value};
use std::io::{Cursor, Read};
const MAX: usize = 2 * 1024 * 1024;
pub(crate) const PDF_PAGED_SOURCE: &str = "file_import_pdf_pages_v1";
pub(crate) const PDF_PAGE_HEADER: &str = "PDF 分页正文\n\n";
pub fn parse(body: &Value) -> Result<Value> {
    let name = body["name"].as_str().ok_or_else(|| anyhow!("缺少文件名"))?;
    let encoded = body["data"]
        .as_str()
        .filter(|s| s.len() <= MAX.div_ceil(3) * 4)
        .ok_or_else(|| anyhow!("文件超过 2 MiB"))?;
    let bytes = STANDARD.decode(encoded)?;
    if bytes.len() > MAX {
        return Err(anyhow!("文件超过 2 MiB"));
    }
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    let text = match ext.as_str() {
        "pdf" => {
            if !bytes.starts_with(b"%PDF-") {
                return Err(anyhow!("文件不是 PDF"));
            }
            let pages = pdf_extract::extract_text_from_mem_by_pages(&bytes)
                .map_err(|e| anyhow!("PDF 正文解析失败:{e}"))?;
            if pages.iter().all(|p| p.trim().is_empty()) {
                return Err(anyhow!("没有提取到文字;扫描版 PDF 需先做 OCR"));
            }
            // Preserve blank pages and neutralize embedded form feeds so source text
            // cannot change the original page numbering. The header survives trim().
            format!(
                "{PDF_PAGE_HEADER}{}",
                pages
                    .iter()
                    .map(|p| p.replace('\u{c}', "\n"))
                    .collect::<Vec<_>>()
                    .join("\u{c}")
            )
        }
        "docx" => {
            let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
            let mut file = zip.by_name("word/document.xml")?;
            if file.size() > 8 * 1024 * 1024 {
                return Err(anyhow!("Word 解压正文过大"));
            }
            let mut xml = String::new();
            file.by_ref()
                .take(8 * 1024 * 1024 + 1)
                .read_to_string(&mut xml)?;
            if xml.len() > 8 * 1024 * 1024 {
                return Err(anyhow!("Word 正文过大"));
            }
            let root = roxmltree::Document::parse(&xml)?;
            let mut text = String::new();
            for p in root
                .descendants()
                .filter(|n| n.is_element() && n.tag_name().name() == "p")
            {
                for t in p
                    .descendants()
                    .filter(|n| n.is_element() && n.tag_name().name() == "t")
                {
                    text.push_str(t.text().unwrap_or(""));
                }
                text.push('\n');
            }
            text
        }
        "xlsx" | "xls" | "xlsb" | "ods" => {
            // Container inflation is bounded before handing ZIP-based sheets to calamine.
            if bytes.starts_with(b"PK") {
                let mut archive = zip::ZipArchive::new(Cursor::new(bytes.as_slice()))?;
                let mut total = 0u64;
                for i in 0..archive.len() {
                    total = total.saturating_add(archive.by_index(i)?.size());
                    if total > 16 * 1024 * 1024 {
                        return Err(anyhow!("表格解压内容超过 16 MiB"));
                    }
                }
            }
            let mut book = open_workbook_auto_from_rs(Cursor::new(bytes))?;
            let mut text = String::new();
            for name in book.sheet_names().to_vec() {
                let range = book.worksheet_range(&name)?;
                if range.height() > 20000 || range.width() > 200 {
                    return Err(anyhow!("表格过大,请拆分后导入"));
                }
                text.push_str(&format!("## 工作表:{name}\n"));
                for row in range.rows() {
                    text.push_str(
                        &row.iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join("\t"),
                    );
                    text.push('\n');
                    if text.len() > 1_000_000 {
                        return Err(anyhow!("表格正文过长,请拆分"));
                    }
                }
            }
            text
        }
        _ => {
            return Err(anyhow!(
                "支持 PDF、DOCX、XLSX、XLS、XLSB、ODS;旧版 DOC 请另存为 DOCX"
            ))
        }
    };
    if text.trim().is_empty() {
        return Err(anyhow!(
            "没有提取到文字;扫描版 PDF 需先做 OCR,不会把空文档当作成功"
        ));
    }
    if text.chars().count() > 250000 {
        return Err(anyhow!("正文超过 250000 字,请拆分;没有截断原文"));
    }
    Ok(
        json!({"body":text,"name":name,"method":ext,"source":if ext=="pdf" {PDF_PAGED_SOURCE}else{"file_import"},"local_only":true}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pdf_original_pages_survive_import_backup_and_citation_checks() {
        use pdf_extract::{dictionary, Document, Object, Stream};
        let mut pdf = Document::with_version("1.5");
        let pages = pdf.new_object_id();
        let font =
            pdf.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
        let resources = pdf.add_object(dictionary! {"Font"=>dictionary!{"F1"=>font}});
        let mut ids = Vec::<Object>::new();
        for content in [
            "",
            "BT /F1 12 Tf 40 750 Td (Revenue verified on page two) Tj ET",
        ] {
            let stream = pdf.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
            let page=pdf.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),600.into(),800.into()],"Resources"=>resources,"Contents"=>stream});
            ids.push(page.into());
        }
        pdf.objects.insert(
            pages,
            dictionary! {"Type"=>"Pages","Kids"=>ids,"Count"=>2}.into(),
        );
        let catalog = pdf.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
        pdf.trailer.set("Root", catalog);
        let mut bytes = Vec::new();
        pdf.save_to(&mut bytes).unwrap();
        let parsed = parse(&json!({"name":"two-pages.pdf","data":STANDARD.encode(bytes)})).unwrap();
        assert_eq!(parsed["source"], PDF_PAGED_SOURCE);
        let root = std::env::temp_dir().join(format!(
            "pdf-pages-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let app = crate::AppCore::new(root.clone()).unwrap();
        let saved=crate::documents::save(app.db_path(),&json!({"kind":"report","title":"Synthetic PDF","body":parsed["body"],"source":parsed["source"]})).unwrap();
        let backup = crate::backup::export(app.db_path()).unwrap();
        crate::backup::restore(
            app.db_path(),
            &json!({"mode":"replace","confirm":true,"backup":backup}),
        )
        .unwrap();
        let result =
            crate::chat_documents::search(app.db_path(), &json!({"query":"Revenue"})).unwrap();
        assert_eq!(result["hits"].as_array().unwrap().len(), 1);
        assert_eq!(result["hits"][0]["page"], 2);
        assert!(!result["hits"][0]["excerpt"]
            .as_str()
            .unwrap()
            .contains('\u{c}'));
        let records = vec![json!({"tool":"search_documents","ok":true,"result":result})];
        let id = saved["id"].as_i64().unwrap();
        assert!(crate::chat_documents::validate(&format!("[资料:{id} p.2]"), &records).is_ok());
        for answer in [
            format!("[资料:{id}]"),
            format!("[资料:{id} p.1]"),
            format!("[资料:{id} p.9]"),
            "无引用".into(),
        ] {
            assert!(crate::chat_documents::validate(&answer, &records).is_err());
        }
        drop(app);
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn invalid_or_oversized_inputs_do_not_import() {
        assert!(parse(&json!({"name":"a.pdf","data":STANDARD.encode(b"not pdf")})).is_err());
        assert!(parse(&json!({"name":"a.exe","data":""})).is_err());
        assert!(parse(&json!({"name":"a.docx","data":"%%%"})).is_err());
    }
    #[test]
    fn docx_text_is_kept_in_paragraph_order() {
        use std::io::Write;
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(
                "word/document.xml",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(b"<w:document xmlns:w=\"word\"><w:p><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p></w:document>").unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        let out = parse(&json!({"name":"sample.docx","data":STANDARD.encode(bytes)})).unwrap();
        assert_eq!(out["body"], "one\ntwo\n");
    }
}
