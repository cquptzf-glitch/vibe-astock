import assert from "node:assert/strict";
import test from "node:test";
import { createElement } from "react";
import { textOf, withRenderer } from "./helpers/render.ts";

test("个股内容首屏只挂载行情，资料和资金通过命名页签访问",async()=>{
 await withRenderer(async r=>{const {StockWorkspace}=await r.load("/src/mobile/StockWorkspace.tsx");
 const html=r.html(StockWorkspace,{quote:createElement("div",null,"行情样本"),company:createElement("div",null,"延迟加载公司资料"),insights:createElement("div",null,"延迟加载资金")});
 assert.match(textOf(html),/行情图表.*公司资料.*量价资金.*行情样本/);
 assert.doesNotMatch(textOf(html),/延迟加载/);
 assert.equal((html.match(/role="tab"/g)||[]).length,3);
 assert.equal((html.match(/role="tabpanel"/g)||[]).length,3);
 });
});
test("设置分区使用按钮，避免锚点覆盖移动路由",async()=>{
 await withRenderer(async r=>{const {PageSections}=await r.load("/src/mobile/PageSections.tsx");const html=r.html(PageSections,{label:"设置分区",items:[{id:"appearance",label:"外观"},{id:"backup",label:"备份"}]});assert.match(html,/aria-label="设置分区"/);assert.match(textOf(html),/外观.*备份/);assert.doesNotMatch(html,/href=/);});
});
test("新增事件、提醒与候选筛选默认折叠，保留明确入口",async()=>{
 await withRenderer(async r=>{for(const [file,name,label] of [["CalendarPanel","CalendarPanel","新增日历事件"],["ReminderPanel","ReminderPanel","新建提醒"],["CandidatePanel","CandidatePanel","筛选条件"]]){const mod=await r.load(`/src/mobile/${file}.tsx`);const html=r.html(mod[name],{onOpenSymbol:()=>{}});assert.match(html,new RegExp(`<summary[^>]*>[\\s\\S]*?${label}`));assert.match(textOf(html),/保存规则|加入日历|筛选条件/);}});
});
