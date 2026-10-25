export function PageSections({ label, items }: { label: string; items: { id: string; label: string; open?: () => void }[] }) {
  return <nav className="page-sections" aria-label={label}>{items.map((item) => <button type="button" key={item.id} onClick={() => {
    item.open?.();
    requestAnimationFrame(() => document.getElementById(item.id)?.scrollIntoView({ block: "start", behavior: "smooth" }));
  }}>{item.label}</button>)}</nav>;
}
