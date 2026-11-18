// 16 px stroke icons for the prototypes (currentColor, 1.35 stroke, square-ish
// terminals to read as "engineered" rather than "friendly").
import type { SVGProps } from "react";

type P = SVGProps<SVGSVGElement>;
const s = (p: P) => ({
  viewBox: "0 0 16 16",
  width: 16,
  height: 16,
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.35,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
  ...p,
});

export const Chevron = (p: P) => (<svg {...s(p)}><path d="M6 4.5L9.5 8 6 11.5" /></svg>);
export const ChevronDown = (p: P) => (<svg {...s(p)}><path d="M4.5 6L8 9.5 11.5 6" /></svg>);
export const Folder = (p: P) => (<svg {...s(p)}><path d="M2 4.2c0-.4.3-.7.7-.7h3.1l1.4 1.4h6.1c.4 0 .7.3.7.7v6.7c0 .4-.3.7-.7.7H2.7a.7.7 0 01-.7-.7z" /></svg>);
export const FolderOpen = (p: P) => (<svg {...s(p)}><path d="M2 11.8V4.2c0-.4.3-.7.7-.7h3.1l1.4 1.4h5.1c.4 0 .7.3.7.7V7" /><path d="M2.3 12.5l1.5-4.6c.1-.3.4-.5.7-.5h9.2l-1.6 4.7a.7.7 0 01-.7.4z" /></svg>);
export const File = (p: P) => (<svg {...s(p)}><path d="M4 1.8h4.8L12 5v8.5c0 .4-.3.7-.7.7H4.7a.7.7 0 01-.7-.7z" /><path d="M8.6 1.9V5.2H12" /></svg>);
export const Close = (p: P) => (<svg {...s(p)}><path d="M4.75 4.75l6.5 6.5M11.25 4.75l-6.5 6.5" /></svg>);
export const Play = (p: P) => (<svg {...s(p)}><path d="M5 3.6v8.8l7-4.4z" fill="currentColor" stroke="none" /></svg>);
export const Stop = (p: P) => (<svg {...s(p)}><rect x="4.2" y="4.2" width="7.6" height="7.6" rx="1" fill="currentColor" stroke="none" /></svg>);
export const Rerun = (p: P) => (<svg {...s(p)}><path d="M12.6 4.4V7.2H9.8" /><path d="M12.3 7A4.6 4.6 0 103.6 9.6" /></svg>);
export const Search = (p: P) => (<svg {...s(p)}><circle cx="7" cy="7" r="4.2" /><path d="M10.2 10.2l3.6 3.6" /></svg>);
export const Files = (p: P) => (<svg {...s(p)}><path d="M5 2h4.5L13 5.5V12a1 1 0 01-1 1H5a1 1 0 01-1-1V3a1 1 0 011-1z" /><path d="M2 5v8.5A1.5 1.5 0 003.5 15H10" /></svg>);
export const Layers = (p: P) => (<svg {...s(p)}><path d="M8 2l6 3.2-6 3.2-6-3.2z" /><path d="M2 8.4l6 3.2 6-3.2" /><path d="M2 11.4l6 3.2 6-3.2" opacity=".55" /></svg>);
export const Agent = (p: P) => (<svg {...s(p)}><rect x="2.5" y="3" width="11" height="8" rx="2" /><path d="M5.5 11v2.5L8.5 11" /><path d="M5.6 7h.01M8 7h.01M10.4 7h.01" strokeWidth="1.8" /></svg>);
export const Crosshair = (p: P) => (<svg {...s(p)}><circle cx="8" cy="8" r="5" /><path d="M8 1.5v3M8 11.5v3M1.5 8h3M11.5 8h3" /></svg>);
export const Gear = (p: P) => (<svg {...s(p)}><circle cx="8" cy="8" r="2.1" /><path d="M8 1.8v1.7M8 12.5v1.7M14.2 8h-1.7M3.5 8H1.8M12.4 3.6l-1.2 1.2M4.8 11.2l-1.2 1.2M12.4 12.4l-1.2-1.2M4.8 4.8L3.6 3.6" /></svg>);
export const Sidebar = (p: P) => (<svg {...s(p)}><rect x="2" y="3" width="12" height="10" rx="1.3" /><path d="M6 3v10" /></svg>);
export const PanelBottom = (p: P) => (<svg {...s(p)}><rect x="2" y="3" width="12" height="10" rx="1.3" /><path d="M2 9.5h12" /></svg>);
export const PanelRight = (p: P) => (<svg {...s(p)}><rect x="2" y="3" width="12" height="10" rx="1.3" /><path d="M10 3v10" /></svg>);
export const Check = (p: P) => (<svg {...s(p)}><path d="M3.5 8.4l2.9 2.9 6-6.3" /></svg>);
export const Warn = (p: P) => (<svg {...s(p)}><path d="M8 2.3l6 10.9H2z" /><path d="M8 6.6v3M8 11.3h.01" /></svg>);
export const Err = (p: P) => (<svg {...s(p)}><circle cx="8" cy="8" r="5.6" /><path d="M5.9 5.9l4.2 4.2M10.1 5.9l-4.2 4.2" /></svg>);
export const Shield = (p: P) => (<svg {...s(p)}><path d="M8 1.8l5 1.9v4c0 3-2.2 5.4-5 6.5-2.8-1.1-5-3.5-5-6.5v-4z" /><path d="M5.8 8l1.6 1.6 2.9-3" /></svg>);
export const Flag = (p: P) => (<svg {...s(p)}><path d="M3.5 14V2.5" /><path d="M3.5 3h8l-1.8 2.6L11.5 8.2h-8" /></svg>);
export const Diff = (p: P) => (<svg {...s(p)}><path d="M5 2.5v6M2 5.5h6M2.5 12h5.5" /><rect x="9.5" y="2.5" width="4" height="11" rx="1" /></svg>);
export const Send = (p: P) => (<svg {...s(p)}><path d="M2.5 8h9.5M8.5 4.2L12.5 8l-4 3.8" /></svg>);
export const Plus = (p: P) => (<svg {...s(p)}><path d="M8 3.2v9.6M3.2 8h9.6" /></svg>);
export const More = (p: P) => (<svg {...s(p)}><path d="M3.8 8h.01M8 8h.01M12.2 8h.01" strokeWidth="2" /></svg>);
export const Trash = (p: P) => (<svg {...s(p)}><path d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.6 8.6h5.8l.6-8.6" /></svg>);
export const Copy = (p: P) => (<svg {...s(p)}><rect x="5.2" y="5.2" width="7.8" height="7.8" rx="1.1" /><path d="M3 10.6V3.8c0-.4.4-.8.8-.8h6.8" /></svg>);
export const Download = (p: P) => (<svg {...s(p)}><path d="M8 2.5v7.5M4.8 7l3.2 3.2L11.2 7M3 13.2h10" /></svg>);
export const Menu = (p: P) => (<svg {...s(p)}><path d="M2.8 4.5h10.4M2.8 8h10.4M2.8 11.5h10.4" /></svg>);
export const Terminal = (p: P) => (<svg {...s(p)}><path d="M3 4.5l3 3-3 3M7.5 11.5H13" /></svg>);
export const Problems = (p: P) => (<svg {...s(p)}><circle cx="8" cy="8" r="5.6" /><path d="M8 5v3.3M8 10.8h.01" /></svg>);

/** Axiom mark: an "A" built from a precision triangle and a crossbar notch. */
export const Mark = (p: P) => (
  <svg {...s(p)} strokeWidth={1.5}>
    <path d="M8 1.9l5.9 11.6H2.1z" />
    <path d="M5.1 10.1h5.8" />
  </svg>
);

export function FileGlyph({ name }: { name: string }) {
  const ext = name.includes(".") ? name.split(".").pop()! : "";
  const label = { rs: "rs", ts: "ts", tsx: "tx", md: "md", toml: "tm", json: "js", lock: "lk" }[ext] ?? "";
  return (
    <span className="glyph" data-ext={ext || "none"} aria-hidden>
      {label}
    </span>
  );
}
