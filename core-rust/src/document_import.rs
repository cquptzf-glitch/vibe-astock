use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use calamine::{open_workbook_auto_from_rs, Reader};
use serde_json::{json, Value};
use std::io::{Cursor, Read};
const MAX: usize = 2 * 1024 * 1024;
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
            pdf_extract::extract_text_from_mem(&bytes)
                .map_err(|e| anyhow!("PDF 正文解析失败:{e}"))?
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
    Ok(json!({"body":text,"name":name,"method":ext,"local_only":true}))
}
#[cfg(test)]
mod tests {
    use super::*;
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
