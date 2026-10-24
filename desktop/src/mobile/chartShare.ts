/** Export only an explicitly supplied public chart. No HTML/page screenshots or external assets. */
export interface ChartCardSpec {
  title: string;
  subtitle: string;
  source: string;
  details: string[];
  notes: string[];
}

export interface ChartImage { base64: string; width: number; height: number }

const SVG_NS = "http://www.w3.org/2000/svg";
const TAGS = new Set(["svg", "g", "defs", "clippath", "lineargradient", "radialgradient", "stop", "mask", "pattern", "path", "line", "polyline", "polygon", "rect", "circle", "ellipse", "text", "tspan", "title", "desc"]);
const STYLES = ["fill", "fill-opacity", "fill-rule", "stroke", "stroke-width", "stroke-opacity", "stroke-linecap", "stroke-linejoin", "stroke-dasharray", "stroke-dashoffset", "opacity", "color", "font-family", "font-size", "font-weight", "font-style", "text-anchor", "dominant-baseline", "visibility", "display", "vector-effect", "clip-path", "stop-color", "stop-opacity"];

/** Keep fragment paint references local to the standalone SVG; reject all resource URLs. */
export function localPaint(value: string): string {
  if (!/url\s*\(/i.test(value)) return value;
  const match = value.match(/^url\(["']?#([\w:.-]+)["']?\)$/i);
  if (!match) throw new Error("图表包含外部资源，无法导出。");
  return `url(#${match[1]})`;
}

export function serializeChart(svg: SVGSVGElement): { xml: string; width: number; height: number } {
  if (!svg.isConnected || svg.closest("[data-private]")) throw new Error("当前没有可分享的公开图表。");
  const box = svg.viewBox.baseVal;
  if (![box.x, box.y, box.width, box.height].every(Number.isFinite) || box.width <= 0 || box.height <= 0 || box.width > 10000 || box.height > 10000) throw new Error("图表尺寸无效。");
  const original = [svg, ...svg.querySelectorAll<SVGElement>("*")];
  if (original.length > 12000) throw new Error("图表过大，请缩小显示范围后重试。");
  for (const element of original) {
    if (element.namespaceURI !== SVG_NS || !TAGS.has(element.localName.toLowerCase()) || element.hasAttribute("data-private")) throw new Error("图表包含不可分享的内容。");
    for (const attr of element.attributes) {
      if (/^on/i.test(attr.name) || /href$/i.test(attr.name)) throw new Error("图表包含外部资源，无法导出。");
      if (/url\s*\(/i.test(attr.value)) localPaint(attr.value);
    }
  }
  const clone = svg.cloneNode(true) as SVGSVGElement;
  const cloned = [clone, ...clone.querySelectorAll<SVGElement>("*")];
  original.forEach((element, index) => {
    const target = cloned[index]!;
    // Styles are resolved while attached, so light/dark colors and indicators survive export.
    const style = getComputedStyle(element);
    target.removeAttribute("style");
    target.removeAttribute("class");
    for (const property of STYLES) {
      const value = style.getPropertyValue(property);
      if (value) target.style.setProperty(property, localPaint(value));
    }
    for (const attr of [...target.attributes]) if (attr.name.startsWith("data-")) target.removeAttribute(attr.name);
  });
  clone.setAttribute("xmlns", SVG_NS);
  clone.setAttribute("width", String(box.width));
  clone.setAttribute("height", String(box.height));
  const xml = new XMLSerializer().serializeToString(clone);
  if (xml.length > 2_500_000) throw new Error("图表过大，请缩小显示范围后重试。");
  return { xml, width: box.width, height: box.height };
}

/** Wrap by Unicode code point, preserving full provenance text instead of clipping it. */
export function chartTextLines(text: string, measure: (value: string) => number, width: number): string[] {
  const out: string[] = [];
  for (const paragraph of text.split(/\r?\n/)) {
    let line = "";
    for (const char of paragraph) {
      if (line && measure(line + char) > width) { out.push(line); line = ""; }
      line += char;
    }
    out.push(line);
  }
  return out;
}

export async function renderChartCard(svg: SVGSVGElement, spec: ChartCardSpec): Promise<ChartImage> {
  const chart = serializeChart(svg);
  const allText = [spec.title, spec.subtitle, spec.source, ...spec.details, ...spec.notes];
  if (allText.some((text) => typeof text !== "string") || allText.join("").length > 6000 || spec.details.length + spec.notes.length > 30) throw new Error("图表说明过长，请查看完整页面或报告。");
  const dark = document.documentElement.dataset.theme === "dark";
  const bg = dark ? "#0d131b" : "#ffffff", fg = dark ? "#edf2f8" : "#172536", muted = dark ? "#a1adbd" : "#536477";
  const canvas = document.createElement("canvas");
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("当前环境无法生成图表图片。");
  const width = 1080, padding = 48, inner = width - 2 * padding;
  const blocks: { lines: string[]; size: number; color: string; gap: number }[] = [];
  const add = (text: string, size: number, color: string, gap = 12) => {
    ctx.font = `${size}px sans-serif`;
    blocks.push({ lines: chartTextLines(text, (s) => ctx.measureText(s).width, inner), size, color, gap });
  };
  add(spec.title, 42, fg, 16);
  add(spec.subtitle, 28, muted);
  spec.details.forEach((line) => add(line, 26, fg, 8));
  const headHeight = padding + blocks.reduce((sum, b) => sum + b.lines.length * (b.size + 10) + b.gap, 0);
  const chartHeight = inner * chart.height / chart.width;
  const headBlocks = blocks.length;
  add(`来源：${spec.source || "未提供"}`, 24, muted);
  spec.notes.filter(Boolean).forEach((line) => add(line, 24, muted));
  add("Vibe AStock · 当前图表快照，仅作历史记录与复盘，不构成买卖建议", 22, muted, 0);
  const footerHeight = blocks.slice(headBlocks).reduce((sum, b) => sum + b.lines.length * (b.size + 10) + b.gap, 0);
  const height = Math.ceil(headHeight + chartHeight + 32 + footerHeight + padding);
  if (height > 3200 || chartHeight < 80) throw new Error("图表与说明过长，无法完整生成图片，请缩小显示范围或查看完整报告。");
  canvas.width = width; canvas.height = height;
  ctx.fillStyle = bg; ctx.fillRect(0, 0, width, height);
  ctx.fillStyle = dark ? "#ff9b70" : "#ad3c19"; ctx.fillRect(0, 0, width, 8);
  let y = padding;
  const paint = (block: typeof blocks[number]) => {
    ctx.fillStyle = block.color; ctx.font = `${block.size}px sans-serif`; ctx.textBaseline = "top";
    for (const line of block.lines) { ctx.fillText(line, padding, y); y += block.size + 10; }
    y += block.gap;
  };
  blocks.slice(0, headBlocks).forEach(paint);
  const url = URL.createObjectURL(new Blob([chart.xml], { type: "image/svg+xml;charset=utf-8" }));
  try {
    const image = await new Promise<HTMLImageElement>((resolve, reject) => {
      const img = new Image();
      const timer = setTimeout(() => { img.onload = null; img.onerror = null; reject(new Error("图表图片生成超时，请重试。")); }, 6000);
      img.onload = () => { clearTimeout(timer); resolve(img); };
      img.onerror = () => { clearTimeout(timer); reject(new Error("图表图片生成失败，请重试。")); };
      img.src = url;
    });
    ctx.drawImage(image, padding, y, inner, chartHeight);
    y += chartHeight + 32;
    blocks.slice(headBlocks).forEach(paint);
    const result = canvas.toDataURL("image/png");
    if (!result.startsWith("data:image/png;base64,")) throw new Error("当前环境无法生成 PNG 图片。");
    const base64 = result.slice(result.indexOf(",") + 1);
    if (base64.length > 11_000_000) throw new Error("图片超过 8 MB，请缩小图表显示范围。");
    return { base64, width, height };
  } finally { URL.revokeObjectURL(url); }
}
