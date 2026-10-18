import { useState } from "react";
import { setAiPage } from "./aiPageContext";
import type { MobilePage } from "./mobileRoutes";
import { RowsShareCard } from "./ShareCard";

export const PUBLIC_ACTION_PAGES = new Set<MobilePage>(["market","stock","research","emotion","ladder","firstBoard","seal","themes","samples","marketReview","structure","weekly","history","limitUp","risk","rotation","news","signals","board","sector","backtest"]);

/** Only rendered public text; inputs, personal sections and closed/hidden elements are omitted. */
export function publicPageText(root:HTMLElement):string {
  const walker=document.createTreeWalker(root,NodeFilter.SHOW_TEXT);const lines:string[]=[];
  while(walker.nextNode()) {const node=walker.currentNode;const p=node.parentElement;if(!p||p.closest('[data-private], [data-page-actions], input,textarea,button,script,style,nav'))continue;
    if(!p.getClientRects().length||getComputedStyle(p).visibility==="hidden")continue;
    const text=node.textContent?.trim();if(text)lines.push(text);
  }
  return lines.join("\n").slice(0,16000);
}
export function PublicPageActions({page,onAsk}:{page:MobilePage;onAsk:(q:string)=>void}) {
  const [preview,setPreview]=useState<{title:string;text:string}|null>(null);
  if(!PUBLIC_ACTION_PAGES.has(page))return null;
  const capture=()=>{const root=document.querySelector<HTMLElement>(".mobile-main");if(!root)return null;return {title:root.querySelector("h1,h2")?.textContent?.trim()||"当前页面",text:publicPageText(root)};};
  return <aside className="public-page-actions" data-page-actions="true" aria-label="当前页面助手与分享">
    <div className="public-page-buttons"><button type="button" onClick={()=>{const p=capture();if(p?.text){setAiPage(p.title,p.text);onAsk(`请解释当前「${p.title}」页面的读数、口径与资料缺口，只描述事实，不给买卖建议。`);}}}>✦ 问 AI</button><button type="button" onClick={()=>setPreview(capture())}>分享图</button></div>
    {preview&&<div className="public-share-preview"><button type="button" onClick={()=>setPreview(null)}>关闭预览</button><RowsShareCard title={preview.title} subtitle={new Date().toLocaleString("zh-CN")} slug={`page-${page}`} hint="公开页面文字摘要；最多 600 字，分享前请核对预览" rows={Array.from(preview.text.replace(/\s+/g," ")).slice(0,600).join("").match(/.{1,20}/gu)?.map((value,i)=>({label:String(i+1),value}))??[]} /></div>}
  </aside>;
}
