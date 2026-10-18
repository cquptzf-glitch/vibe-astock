import assert from "node:assert/strict";
import test from "node:test";
import { withRenderer, textOf } from "./helpers/render.ts";
const fact={sector:"hbm",metric:"yield",entity:"合成测试厂",label:"良率",value:80,unit:"%",currency:"none",period_end:"2026-06-30",published_date:"2026-07-01",source_name:"合成公告",source_url:"https://example.com/report",source_excerpt:"合成数据：良率80%。"};
test("产业观测显示单位、统计期、出处和未核验状态",async()=>{
  await withRenderer(async r=>{const {IndustryFactsView}=await r.load("/src/mobile/IndustryLibrary.tsx");const html=r.html(IndustryFactsView,{records:[fact]});const text=textOf(html);for(const s of ["80 %","2026-06-30","2026-07-01","合成公告","合成数据","用户整理，未独立核验"])assert.ok(text.includes(s),s);assert.ok(html.includes('https://example.com/report'));});
});
test("导入预览拒绝缺失数值、其他赛道及非HTTPS链接",async()=>{
  await withRenderer(async r=>{const {parseIndustryPack}=await r.load("/src/mobile/IndustryLibrary.tsx");const encode=(record:unknown)=>JSON.stringify({schema:1,records:[record]});assert.equal(parseIndustryPack(encode(fact),"hbm")[0].value,80);
    for(const f of [{...fact,value:null},{...fact,value:"80"},{...fact,source_url:"javascript:alert(1)"},{...fact,sector:"pcb"},{...fact,source_excerpt:""}])assert.throws(()=>parseIndustryPack(encode(f),"hbm"));
  });
});
