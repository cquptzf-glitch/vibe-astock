import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { MobileApp } from "./mobile/MobileApp";
import { PRODUCT_NAME } from "./verticals/finance/lib/brand";
import "./index.css";
document.title = PRODUCT_NAME;
const root = document.getElementById("root");
if (!root) throw new Error("找不到 #root");
createRoot(root).render(<StrictMode><MobileApp /></StrictMode>);
