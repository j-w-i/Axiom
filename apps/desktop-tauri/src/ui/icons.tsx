// A small, hand-drawn 16 px icon set (stroke icons, currentColor). Kept local
// to avoid an icon-library dependency for the prototype.
import type { SVGProps } from "react";

type P = SVGProps<SVGSVGElement>;
const base = (props: P) => ({
  viewBox: "0 0 16 16",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.4,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
  ...props,
});

export const Chevron = (p: P) => (<svg {...base(p)}><path d="M6 4l4 4-4 4" /></svg>);
export const Folder = (p: P) => (<svg {...base(p)}><path d="M2 4.5a1 1 0 011-1h3l1.5 1.5H13a1 1 0 011 1V12a1 1 0 01-1 1H3a1 1 0 01-1-1z" /></svg>);
export const FolderOpen = (p: P) => (<svg {...base(p)}><path d="M2 12V4.5a1 1 0 011-1h3l1.5 1.5H12a1 1 0 011 1V7" /><path d="M2.5 13l1.6-5.2a1 1 0 011-.8H14l-1.7 5.3a1 1 0 01-1 .7z" /></svg>);
export const File = (p: P) => (<svg {...base(p)}><path d="M4 2h5l3 3v8.5a.5.5 0 01-.5.5h-7a.5.5 0 01-.5-.5z" /><path d="M9 2v3h3" /></svg>);
export const Link = (p: P) => (<svg {...base(p)}><path d="M7 9a2.5 2.5 0 003.5 0l2-2A2.5 2.5 0 009 3.5l-.5.5" /><path d="M9 7a2.5 2.5 0 00-3.5 0l-2 2A2.5 2.5 0 007 12.5l.5-.5" /></svg>);
export const Close = (p: P) => (<svg {...base(p)}><path d="M4.5 4.5l7 7M11.5 4.5l-7 7" /></svg>);
export const Play = (p: P) => (<svg {...base(p)}><path d="M5 3.5v9l7-4.5z" fill="currentColor" /></svg>);
export const Stop = (p: P) => (<svg {...base(p)}><rect x="4" y="4" width="8" height="8" rx="1" fill="currentColor" /></svg>);
export const Trash = (p: P) => (<svg {...base(p)}><path d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.5 8.5h6l.5-8.5" /></svg>);
export const Copy = (p: P) => (<svg {...base(p)}><rect x="5" y="5" width="8" height="8" rx="1.2" /><path d="M3 10.5V3.8A.8.8 0 013.8 3h6.7" /></svg>);
export const Sidebar = (p: P) => (<svg {...base(p)}><rect x="2" y="3" width="12" height="10" rx="1.5" /><path d="M6 3v10" /></svg>);
export const Panel = (p: P) => (<svg {...base(p)}><rect x="2" y="3" width="12" height="10" rx="1.5" /><path d="M2 9.5h12" /></svg>);
export const Gear = (p: P) => (<svg {...base(p)}><circle cx="8" cy="8" r="2" /><path d="M8 1.8v1.6M8 12.6v1.6M14.2 8h-1.6M3.4 8H1.8M12.4 3.6l-1.1 1.1M4.7 11.3l-1.1 1.1M12.4 12.4l-1.1-1.1M4.7 4.7L3.6 3.6" /></svg>);
export const Search = (p: P) => (<svg {...base(p)}><circle cx="7" cy="7" r="4" /><path d="M10 10l3.5 3.5" /></svg>);
export const Refresh = (p: P) => (<svg {...base(p)}><path d="M13 4v3h-3" /><path d="M12.6 7A5 5 0 103 9" /></svg>);
export const Files = (p: P) => (<svg {...base(p)}><path d="M5 4.5V2.8a.8.8 0 01.8-.8h4.2l3 3v7.2a.8.8 0 01-.8.8H11" /><path d="M3 5.3a.8.8 0 01.8-.8H8l3 3v6.7a.8.8 0 01-.8.8H3.8a.8.8 0 01-.8-.8z" /></svg>);
export const Layers = (p: P) => (<svg {...base(p)}><path d="M8 2l6 3-6 3-6-3z" /><path d="M2 8l6 3 6-3" /><path d="M2 11l6 3 6-3" /></svg>);
export const Check = (p: P) => (<svg {...base(p)}><path d="M3.5 8.5l3 3 6-7" /></svg>);
export const Download = (p: P) => (<svg {...base(p)}><path d="M8 2.5v8M4.8 7.5L8 10.7l3.2-3.2" /><path d="M3 13h10" /></svg>);
export const Archive = (p: P) => (<svg {...base(p)}><rect x="2" y="3" width="12" height="3" rx=".8" /><path d="M3 6v6.5a.5.5 0 00.5.5h9a.5.5 0 00.5-.5V6M6.5 8.5h3" /></svg>);
export const Terminal = (p: P) => (<svg {...base(p)}><rect x="2" y="3" width="12" height="10" rx="1.2" /><path d="M4.5 6.5l2 1.5-2 1.5M8 10h3" /></svg>);
export const Shield = (p: P) => (<svg {...base(p)}><path d="M8 2l5 2v3.5c0 3-2.2 5.3-5 6.5-2.8-1.2-5-3.5-5-6.5V4z" /></svg>);
export const Warning = (p: P) => (<svg {...base(p)}><path d="M8 2.5l6 11H2z" /><path d="M8 6.5v3M8 11.5v.2" /></svg>);
export const Dock = (p: P) => (<svg {...base(p)}><rect x="2" y="3" width="12" height="10" rx="1.5" /><path d="M10 3v10" /></svg>);
export const Chat = (p: P) => (<svg {...base(p)}><path d="M2.5 4a1 1 0 011-1h9a1 1 0 011 1v6a1 1 0 01-1 1H7l-3 2.5V11h-.5a1 1 0 01-1-1z" /></svg>);
export const Plus = (p: P) => (<svg {...base(p)}><path d="M8 3v10M3 8h10" /></svg>);
export const Send = (p: P) => (<svg {...base(p)}><path d="M2.5 8L13.5 3l-3 10.5-2.5-4.5z" /><path d="M8 9l5.5-6" /></svg>);
export const Image = (p: P) => (<svg {...base(p)}><rect x="2" y="3" width="12" height="10" rx="1.2" /><circle cx="6" cy="6.5" r="1.2" /><path d="M2.5 12l3.5-3.5 2.5 2.5 2-2 3 3" /></svg>);
export const Edit = (p: P) => (<svg {...base(p)}><path d="M3 13l.6-2.6L10.5 3.5l2 2-6.9 6.9z" /><path d="M9.5 4.5l2 2" /></svg>);
export const More = (p: P) => (<svg {...base(p)}><circle cx="4" cy="8" r=".6" fill="currentColor" /><circle cx="8" cy="8" r=".6" fill="currentColor" /><circle cx="12" cy="8" r=".6" fill="currentColor" /></svg>);
export const ArrowDown = (p: P) => (<svg {...base(p)}><path d="M8 3v10M4 9l4 4 4-4" /></svg>);
export const Collapse = (p: P) => (<svg {...base(p)}><path d="M5 7l3-3 3 3M5 12l3-3 3 3" /></svg>);
// The Axiom mark, a drafting divider (pivot, two legs, arc of travel) that reads
// as an A. Source of the app icons: src-tauri/icons/source/ (keep in sync).
export const Logo = (p: P) => (<svg {...base(p)} strokeWidth={1.5}><circle cx="8" cy="3" r="1.5" /><path d="M7.3 4.35L3.25 14M8.7 4.35l4.05 9.65" /><path d="M4.6 10.75Q8 12.4 11.4 10.75" /></svg>);
