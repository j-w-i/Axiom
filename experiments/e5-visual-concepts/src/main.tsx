// Entry: ?c=a|b|c&theme=dark|light&view=workspace|environments|settings|components
// &palette=commands|files&toast&still. Without ?c, an index of every state.
import "@fontsource/ibm-plex-sans/400.css";
import "@fontsource/ibm-plex-sans/500.css";
import "@fontsource/ibm-plex-sans/600.css";
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/ibm-plex-mono/500.css";
import "@fontsource/barlow/400.css";
import "@fontsource/barlow/500.css";
import "@fontsource/barlow/600.css";
import "@fontsource/barlow-semi-condensed/500.css";
import "@fontsource/barlow-semi-condensed/600.css";
import "@fontsource/instrument-sans/400.css";
import "@fontsource/instrument-sans/500.css";
import "@fontsource/instrument-sans/600.css";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
import "@fontsource/jetbrains-mono/700.css";
import "./base.css";
import { createRoot } from "react-dom/client";
import { ConceptA } from "./concepts/a";
import { ConceptB } from "./concepts/b";
import { ConceptC } from "./concepts/c";
import { useWorkbench } from "./workbench";

function App() {
  const wb = useWorkbench();
  if (wb.concept === "b") return <ConceptB wb={wb} />;
  if (wb.concept === "c") return <ConceptC wb={wb} />;
  if ((wb.concept as string) === "r") return <ConceptA wb={wb} variant="r" />;
  return <ConceptA wb={wb} />;
}

function Index() {
  const names = { a: "A — Precision Workstation", b: "B — Creative Instrument", c: "C — Modern Technical Studio", r: "Recommended — Precision Instrument (A + B + C)" };
  const views = ["workspace", "environments", "settings", "components"];
  return (
    <div style={{ font: "14px system-ui", padding: 24, overflow: "auto", height: "100%" }}>
      <h1 style={{ fontSize: 18 }}>Axiom M1.6 visual concepts (isolated prototypes)</h1>
      {(["r", "a", "b", "c"] as const).map((c) => (
        <section key={c} style={{ margin: "16px 0" }}>
          <h2 style={{ fontSize: 15 }}>{names[c]}</h2>
          {(["dark", "light"] as const).map((t) => (
            <p key={t}>
              {t}:{" "}
              {views.map((v) => <a key={v} style={{ marginRight: 12 }} href={`?c=${c}&theme=${t}&view=${v}`}>{v}</a>)}
              <a href={`?c=${c}&theme=${t}&palette=commands`}>palette</a>
            </p>
          ))}
        </section>
      ))}
    </div>
  );
}

const root = createRoot(document.getElementById("root")!);
root.render(new URLSearchParams(location.search).has("c") ? <App /> : <Index />);
