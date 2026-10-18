//! Public, anonymous BaoStock protocol; no Python runtime, account or model tool.
use anyhow::{anyhow, ensure, Result};
use chrono::{Duration as Days, NaiveDate};
use serde_json::{json, Value};
use std::{collections::BTreeMap, io::Read, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

const SEP: char = '\u{1}';
const END: &[u8] = b"<![CDATA[]]>\n";
const FIELDS: &str = "date,code,tradestatus,isST";
#[derive(Clone, Debug)]
pub(crate) struct Day {
    pub tradable: bool,
    pub st: bool,
}
#[derive(Debug)]
pub(crate) struct History {
    pub ipo: NaiveDate,
    pub delisted: Option<NaiveDate>,
    pub days: BTreeMap<String, Day>,
    pub ipo_days: Vec<String>,
}
fn date(text: &str) -> Result<NaiveDate> {
    let d = NaiveDate::parse_from_str(text, "%Y-%m-%d")?;
    ensure!(d.to_string() == text, "历史元数据日期格式错误");
    Ok(d)
}
fn frame(kind: &str, parts: &[String]) -> Vec<u8> {
    let body = parts.join(&SEP.to_string());
    let data = format!("00.9.40{SEP}{kind}{SEP}{:010}{body}", body.len());
    format!("{data}{SEP}{}\n", crc32fast::hash(data.as_bytes())).into_bytes()
}
fn decode(header: &[u8], payload: &[u8], expected: &str) -> Result<Vec<String>> {
    let h = std::str::from_utf8(header)?;
    let fields: Vec<_> = h.split(SEP).collect();
    ensure!(
        fields.len() == 3 && fields[1] == expected,
        "BaoStock 响应类型错误"
    );
    ensure!(
        fields[2].parse::<usize>()? == payload.len(),
        "BaoStock 响应长度错误"
    );
    let body = if expected == "96" {
        let mut out = Vec::new();
        flate2::read::ZlibDecoder::new(payload)
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut out)?;
        ensure!(out.len() <= 8 * 1024 * 1024, "BaoStock 解压数据超限");
        String::from_utf8(out)?
    } else {
        std::str::from_utf8(payload)?.to_owned()
    };
    let parts: Vec<String> = body
        .trim_end_matches('\n')
        .split(SEP)
        .map(str::to_owned)
        .collect();
    ensure!(
        parts.first().is_some_and(|v| v == "0"),
        "BaoStock 返回失败:{}",
        parts.first().map(String::as_str).unwrap_or("缺状态")
    );
    Ok(parts)
}
struct Session(TcpStream);
impl Session {
    async fn request(
        &mut self,
        kind: &str,
        response: &str,
        parts: &[String],
    ) -> Result<Vec<String>> {
        tokio::time::timeout(Duration::from_secs(12), async {
            self.0.write_all(&frame(kind, parts)).await?;
            let mut header = [0; 21];
            self.0.read_exact(&mut header).await?;
            let h = std::str::from_utf8(&header)?;
            let size = h
                .split(SEP)
                .nth(2)
                .ok_or_else(|| anyhow!("BaoStock 消息头错误"))?
                .parse::<usize>()?;
            ensure!(size <= 4 * 1024 * 1024, "BaoStock 响应超限");
            let mut payload = vec![0; size];
            self.0.read_exact(&mut payload).await?;
            let mut trailer = Vec::new();
            while !trailer.ends_with(END) {
                ensure!(trailer.len() < 64, "BaoStock 响应尾错误");
                trailer.push(self.0.read_u8().await?);
            }
            decode(&header, &payload, response)
        })
        .await
        .map_err(|_| anyhow!("BaoStock 历史元数据请求超时"))?
    }
    async fn connect() -> Result<Self> {
        let stream = tokio::time::timeout(
            Duration::from_secs(8),
            TcpStream::connect(("public-api.baostock.com", 10030)),
        )
        .await??;
        let mut s = Self(stream);
        let r = s
            .request("00", "01", &strings(&["login", "anonymous", "123456", "0"]))
            .await?;
        ensure!(
            r.get(2).is_some_and(|v| v == "login") && r.get(3).is_some_and(|v| v == "anonymous"),
            "BaoStock 登录回执错误"
        );
        Ok(s)
    }
    async fn days(&mut self, code: &str, start: &str, end: &str) -> Result<BTreeMap<String, Day>> {
        let mut out = BTreeMap::new();
        for page in 1..=20 {
            let r = self
                .request(
                    "95",
                    "96",
                    &strings(&[
                        "query_history_k_data_plus",
                        "anonymous",
                        &page.to_string(),
                        "2000",
                        code,
                        FIELDS,
                        start,
                        end,
                        "d",
                        "3",
                    ]),
                )
                .await?;
            ensure!(
                r.get(2).is_some_and(|v| v == "query_history_k_data_plus")
                    && r.get(4) == Some(&page.to_string())
                    && r.get(7).is_some_and(|v| v == code)
                    && r.get(8).is_some_and(|v| v == FIELDS)
                    && r.get(9).is_some_and(|v| v == start)
                    && r.get(10).is_some_and(|v| v == end),
                "BaoStock 分页或查询口径回执不一致"
            );
            let data: Value =
                serde_json::from_str(r.get(6).ok_or_else(|| anyhow!("BaoStock 缺数据"))?)?;
            let rows = data["record"]
                .as_array()
                .ok_or_else(|| anyhow!("BaoStock 缺 record"))?;
            ensure!(rows.len() <= 2000, "BaoStock 分页超过请求条数");
            let batch = parse_days(rows, code, start, end)?;
            for (d, v) in batch {
                ensure!(out.insert(d, v).is_none(), "BaoStock 跨页日期重复");
            }
            if rows.len() < 2000 {
                return Ok(out);
            }
        }
        Err(anyhow!("BaoStock 分页未结束，没有接受不完整历史"))
    }
}
fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}
fn parse_days(rows: &[Value], code: &str, start: &str, end: &str) -> Result<BTreeMap<String, Day>> {
    let mut out = BTreeMap::new();
    for row in rows {
        let a = row
            .as_array()
            .filter(|a| a.len() == 4)
            .ok_or_else(|| anyhow!("BaoStock 日状态列数错误"))?;
        let d = a[0].as_str().ok_or_else(|| anyhow!("BaoStock 缺日期"))?;
        date(d)?;
        ensure!(
            d >= start && d <= end && a[1] == code,
            "BaoStock 日期或代码超出本轮请求"
        );
        let bit = |v: &Value| -> Result<bool> {
            match v.as_str() {
                Some("1") => Ok(true),
                Some("0") => Ok(false),
                _ => Err(anyhow!("BaoStock 状态缺失，不推断非 ST/可交易")),
            }
        };
        ensure!(
            out.insert(
                d.to_owned(),
                Day {
                    tradable: bit(&a[2])?,
                    st: bit(&a[3])?
                }
            )
            .is_none(),
            "BaoStock 日状态重复"
        );
    }
    Ok(out)
}
pub(crate) async fn fetch(key: &crate::SymbolKey, start: &str, end: &str) -> Result<History> {
    ensure!(
        key.market != crate::Market::BJ,
        "BaoStock 不覆盖北交所历史状态"
    );
    date(start)?;
    date(end)?;
    ensure!(start <= end, "历史元数据日期倒置");
    let code = format!("{}.{}", key.market.tencent_prefix(), key.digits);
    let mut s = Session::connect().await?;
    let basic = s
        .request(
            "45",
            "46",
            &strings(&["query_stock_basic", "anonymous", "1", "2000", &code, ""]),
        )
        .await?;
    ensure!(
        basic.get(2).is_some_and(|v| v == "query_stock_basic")
            && basic.get(7).is_some_and(|v| v == &code),
        "BaoStock 公司资料回执错误"
    );
    let fields: Vec<_> = basic
        .get(9)
        .ok_or_else(|| anyhow!("BaoStock 公司资料字段缺失"))?
        .split(',')
        .map(str::trim)
        .collect();
    ensure!(
        fields.starts_with(&["code", "code_name", "ipoDate", "outDate", "type"]),
        "BaoStock 公司资料字段顺序变化"
    );
    let data: Value = serde_json::from_str(
        basic
            .get(6)
            .ok_or_else(|| anyhow!("BaoStock 公司资料缺数据"))?,
    )?;
    let rows = data["record"]
        .as_array()
        .filter(|a| a.len() == 1)
        .ok_or_else(|| anyhow!("BaoStock 公司资料未唯一命中"))?;
    let row = rows[0]
        .as_array()
        .filter(|a| a.len() >= 5)
        .ok_or_else(|| anyhow!("BaoStock 公司资料列缺失"))?;
    ensure!(
        row[0] == code && row[4] == "1",
        "BaoStock 公司资料代码/股票类型不一致"
    );
    let ipo = date(row[2].as_str().unwrap_or(""))?;
    let delisted = match row[3].as_str().unwrap_or("") {
        "" => None,
        d => Some(date(d)?),
    };
    let days = s.days(&code, start, end).await?;
    ensure!(!days.is_empty(), "BaoStock 区间没有日状态");
    let ipo_days = if ipo.to_string().as_str() <= end && ipo + Days::days(60) >= date(start)? {
        let last = (ipo + Days::days(60)).min(date(end)?).to_string();
        let first = s.days(&code, &ipo.to_string(), &last).await?;
        ensure!(
            first.keys().next().is_some_and(|d| d == &ipo.to_string()),
            "BaoStock 上市首日未覆盖，不能推断前五日"
        );
        first.keys().take(5).cloned().collect()
    } else {
        vec![]
    };
    Ok(History {
        ipo,
        delisted,
        days,
        ipo_days,
    })
}
pub(crate) fn provenance(h: &History) -> Value {
    json!({"source":"BaoStock anonymous TCP","source_url":"https://github.com/zxygithub/baostock","ipo_date":h.ipo.to_string(),"delist_date":h.delisted.map(|d|d.to_string()),"daily_states":h.days.len(),"st_days":h.days.values().filter(|d|d.st).count(),"ipo_days":h.ipo_days,"fetched_at":chrono::Utc::now().to_rfc3339()})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn state_rows_never_infer_missing_status_or_accept_wrong_code_and_duplicate_date() {
        let row = json!(["2026-07-03", "sh.600000", "1", "1"]);
        assert!(
            parse_days(&[row.clone()], "sh.600000", "2026-07-01", "2026-07-06").unwrap()
                ["2026-07-03"]
                .st
        );
        for rows in [
            vec![row.clone(), row.clone()],
            vec![json!(["2026-07-03", "sh.600001", "1", "1"])],
            vec![json!(["2026-07-03", "sh.600000", "1", ""])],
            vec![json!(["2026-07-07", "sh.600000", "1", "0"])],
        ] {
            assert!(parse_days(&rows, "sh.600000", "2026-07-01", "2026-07-06").is_err());
        }
    }
    #[test]
    fn wire_frames_and_compressed_payload_are_bounded_and_typed() {
        use std::io::Write;
        let b = frame("00", &strings(&["login", "anonymous", "123456", "0"]));
        assert_eq!(&b[..21], b"00.9.40\x0100\x010000000024");
        let payload = b"0\x01success\x01query_history_k_data_plus";
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(payload).unwrap();
        let zipped = z.finish().unwrap();
        let header = format!("00.9.10\x0196\x01{:010}", zipped.len());
        assert_eq!(
            decode(header.as_bytes(), &zipped, "96").unwrap()[2],
            "query_history_k_data_plus"
        );
        assert!(decode(header.as_bytes(), &zipped, "46").is_err());
        assert!(decode(header.as_bytes(), b"broken", "96").is_err());
    }
    #[tokio::test]
    #[ignore = "public BaoStock TCP connectivity probe"]
    async fn live_historical_metadata_probe() {
        let h = fetch(
            &crate::normalize_stock_symbol("600519").unwrap(),
            "2026-07-01",
            "2026-07-10",
        )
        .await
        .unwrap();
        assert_eq!(h.ipo.to_string(), "2001-08-27");
        assert_eq!(h.days.len(), 8);
        assert!(h.days.values().all(|d| !d.st));
        println!("{}", provenance(&h));
        let st = fetch(
            &crate::normalize_stock_symbol("600221").unwrap(),
            "2021-03-01",
            "2021-03-05",
        )
        .await
        .unwrap();
        assert!(st.days.values().all(|d| d.st));
        println!("historical ST: {}", provenance(&st));
        let ipo = fetch(
            &crate::normalize_stock_symbol("688001").unwrap(),
            "2019-07-22",
            "2019-07-30",
        )
        .await
        .unwrap();
        assert_eq!(
            ipo.ipo_days,
            vec![
                "2019-07-22",
                "2019-07-23",
                "2019-07-24",
                "2019-07-25",
                "2019-07-26"
            ]
        );
        println!("historical IPO: {}", provenance(&ipo));
    }
}
