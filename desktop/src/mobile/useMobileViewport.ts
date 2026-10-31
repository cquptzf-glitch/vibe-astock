import { useEffect } from "react";

/** Keep editing controls above the soft keyboard, without moving reading views. */
export function useMobileViewport() {
  useEffect(() => {
    const viewport = window.visualViewport;
    let baseline = window.innerHeight;
    const update = () => {
      const focused = document.activeElement as HTMLElement | null;
      const editing = focused?.matches('input:not([type="checkbox"]):not([type="radio"]), textarea, [contenteditable="true"]');
      const height = viewport?.height ?? window.innerHeight;
      if (!editing) baseline = window.innerHeight;
      const open = Boolean(editing && baseline - height > 120);
      document.documentElement.dataset.mobileKeyboard = open ? "open" : "closed";
      if (open) focused?.scrollIntoView({ block: "nearest" });
    };
    window.addEventListener("resize", update);
    viewport?.addEventListener("resize", update);
    document.addEventListener("focusin", update);
    document.addEventListener("focusout", update);
    return () => {
      window.removeEventListener("resize", update);
      viewport?.removeEventListener("resize", update);
      document.removeEventListener("focusin", update);
      document.removeEventListener("focusout", update);
      delete document.documentElement.dataset.mobileKeyboard;
    };
  }, []);
}
