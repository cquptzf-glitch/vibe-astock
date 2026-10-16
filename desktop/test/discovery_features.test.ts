import assert from "node:assert/strict";
import test from "node:test";
import { textOf, withRenderer } from "./helpers/render.ts";
import { publicUrl, openPublicUrl } from "../src/mobile/externalLinks.ts";
import { createMobileNavigation, mobilePath, parseMobileRoute, type NavigationHost } from "../src/mobile/mobileRoutes.ts";

test("资讯保留来源、部分失败、缺失时间，过滤与文本转义可见", async () => {
  await withRenderer(async (r) => {
    const { NewsResults } = await r.load("/src/mobile/NewsPanel.tsx");
    const data = { status: "partial", sources_ok: 1, sources_tried: 2,
      failures: [{ source: "源二", message: "请求超时" }], fetched_at: "",
      items: [{ title: "<script>行业样本</script>", summary: "芯片供应", link: null, published: null, source: "源一", redline: ["供应"] }] };
    const html = r.html(NewsResults, { data, onOpen: () => undefined });
    assert.match(textOf(html), /部分来源获取失败.*源二/);
    assert.match(textOf(html), /发布时间未提供/);
    assert.match(html, /&lt;script&gt;/);
    assert.doesNotMatch(html, /<script>/);
    assert.match(textOf(r.html(NewsResults, { data, keyword: "不存在", onOpen: () => undefined })), /所选条件下暂无资讯/);
    assert.match(textOf(r.html(NewsResults, { data, redlineOnly: true, onOpen: () => undefined })), /芯片供应/);
  });
});

test("GPU 现货、缺席和故障分别显示，旧采样不伪装成当前报价", async () => {
  await withRenderer(async (r) => {
    const { GpuRentResults } = await r.load("/src/mobile/GpuRentPanel.tsx");
    const data = { status: "partial", source: "500.farm", method: "分组统计后聚合的中位", guard: "不含电力机房运维", scope: "现货历史版", errors: [],
      items: [
        { gpu: "B200", status: "partial", latest: 6.4, asof_ts: 1791504000, change_7d_pct: null, points: [[1790812800, 6], [1791504000, 6.4]], message: "最新采样已超过 3 天", stale: true, depreciation_reference_usd: 3 },
        { gpu: "H100 SXM", status: "unavailable", latest: null, points: [], message: "暂无序列；不等于租金为零" },
        { gpu: "A100 SXM4", status: "failed", latest: null, points: [], message: "数据源不可用" },
      ] };
    const html = r.html(GpuRentResults, { data });
    const text = textOf(html);
    assert.match(text, /6.4/);
    assert.match(text, /最新采样已超过 3 天/);
    assert.match(text, /暂无序列；不等于租金为零/);
    assert.match(text, /数据源不可用/);
    assert.match(text, /分组统计后聚合的中位/);
    assert.match(html, /role="img"/);
  });
});

test("板块概览缺失时仍展示成分股，明确分页统计范围与缺值", async () => {
  await withRenderer(async (r) => {
    const { BoardDetailResults } = await r.load("/src/mobile/BoardDetailPanel.tsx");
    const data = { status: "partial", code: "BK0475", quote: null, history: null, source: "eastmoney",
      errors: [{source:"板块概览",message:"超时"}],
      constituents: {items:[{symbol:"BJ430047",name:"样本公司",price:null,change_pct:null,amount_yuan:null,turnover_pct:null,main_net_yuan:null}],total:120,page:1,has_more:true,dropped:0,page_breadth:{up:0,down:0,flat:0,unknown:1}} };
    const text = textOf(r.html(BoardDetailResults, { data, onOpenSymbol: () => undefined }));
    assert.match(text, /成分股总数 120 只/);
    assert.match(text, /仅覆盖当前页，非全板块统计/);
    assert.match(text, /BJ430047/);
    assert.match(text, /主力净额 —/);
    assert.match(text, /部分数据暂不可用/);
  });
});

test("资讯与信号有独立路由，板块打开个股后按来源返回", () => {
  assert.deepEqual(parseMobileRoute("/mobile/discovery/news"), {page:"news"});
  assert.deepEqual(parseMobileRoute("/mobile/discovery/signals"), {page:"signals"});
  assert.deepEqual(parseMobileRoute("/mobile/board/bk0475"), {page:"board",board:"BK0475"});
  assert.equal(parseMobileRoute("/mobile/board/BK0475?url=evil"), null);
  assert.equal(mobilePath({page:"board",board:"../settings"}), "/mobile/market");
  const entries: {hash:string;state:unknown}[] = [{hash:"",state:null}];
  let index=0;
  const events=new Map<string,Set<()=>void>>();
  const host: NavigationHost = {
    location:{get hash(){return entries[index]!.hash;}},
    history:{get state(){return entries[index]!.state;},
      pushState(state,_unused,hash){entries.splice(++index);entries.push({hash,state});},
      replaceState(state,_unused,hash){entries[index]={hash,state};},
      back(){if(index>0){index--;for(const fn of events.get("popstate")??[])fn();}}},
    addEventListener(type,fn){if(!events.has(type))events.set(type,new Set());events.get(type)!.add(fn);},
    removeEventListener(type,fn){events.get(type)?.delete(fn);},
  };
  const nav=createMobileNavigation(host);
  const stop=nav.subscribe(()=>undefined);
  nav.navigate({page:"rotation"});nav.navigate({page:"board",board:"BK0475"});nav.navigate({page:"stock",symbol:"SH600519"});
  nav.back();assert.equal(nav.snapshot(),"/mobile/board/BK0475");
  nav.back();assert.equal(nav.snapshot(),"/mobile/discovery/rotation");
  stop();
});

test("资讯外链只接受网页 URL，通过原生 opener 打开且不导航业务 WebView", async () => {
  for(const raw of ["javascript:alert(1)","file:///etc/passwd","https://user:password@example.com","vibeastock://settings"]){
    assert.equal(publicUrl(raw),null);
  }
  const previous=Object.getOwnPropertyDescriptor(globalThis,"window");
  const calls: unknown[]=[];
  Object.defineProperty(globalThis,"window",{configurable:true,value:{__TAURI__:{core:{invoke:async(command:string,args:unknown)=>{calls.push([command,args]);}}}}});
  try {
    await openPublicUrl("https://example.com/article");
    assert.deepEqual(calls,[["plugin:opener|open_url",{url:"https://example.com/article"}]]);
    await assert.rejects(openPublicUrl("javascript:alert(1)"));
    assert.equal(calls.length,1);
  } finally {
    if(previous)Object.defineProperty(globalThis,"window",previous);else Reflect.deleteProperty(globalThis,"window");
  }
});
