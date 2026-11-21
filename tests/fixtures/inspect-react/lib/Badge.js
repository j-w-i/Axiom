// A "library" component: plain createElement, no JSX, so the plugin does not stamp it. The
// inspector can only name its owner (App.jsx) as an uncertain candidate.
import { createElement } from "react";

export function Badge({ label }) {
  return createElement("em", { className: "badge" }, label);
}
