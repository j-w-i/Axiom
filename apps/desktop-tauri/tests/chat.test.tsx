// Chat views (M3-UI-001): Markdown rendering rules (GFM, code blocks, no raw HTML, links to the
// host, no remote images) and the pure helpers.

import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { listTarget, MarkdownView, readImage, relativeTime } from "../src/components/Chat";

const md = (text: string) => renderToStaticMarkup(<MarkdownView text={text} dark onOpenLink={() => {}} />);

describe("chat markdown", () => {
  it("renders GFM: tables, task lists, strikethrough, autolinks", () => {
    const html = md("| a | b |\n|---|---|\n| 1 | 2 |\n\n- [x] done\n- [ ] todo\n\n~~old~~ https://example.com");
    expect(html).toContain("<table>");
    expect(html).toContain("<td>1</td>");
    expect(html).toMatch(/<input[^>]*checked=""[^>]*disabled=""|<input[^>]*disabled=""[^>]*checked=""/);
    expect(html).toContain("<del>old</del>");
    expect(html).toContain('href="https://example.com"');
  });

  it("renders a fenced block as a code block with its language and a Copy button", () => {
    const html = md("Here:\n\n```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n\nand `inline`.");
    expect(html).toContain('class="code-block"');
    expect(html).toContain('<span class="code-block__lang">rust</span>');
    expect(html).toContain("fn main() {");
    expect(html).toContain('aria-label="Copy code"');
    expect(html).toContain('<code class="md__code">inline</code>');
  });

  it("never renders raw HTML from the model, nor loads remote images", () => {
    const html = md('<script>alert(1)</script><img src="x" onerror="alert(2)"> <b>bold</b>\n\n![tracker](https://evil.example/p.gif)');
    // Shown as escaped text, never as elements.
    expect(html).not.toContain("<script");
    expect(html).toContain("&lt;script&gt;");
    expect(html).not.toMatch(/<img[^>]*onerror/);
    expect(html).not.toContain("<b>");
    expect(html).not.toMatch(/<img[^>]*evil/);
    expect(html).toContain("[image: tracker]");
  });
});

describe("chat helpers", () => {
  it("relative times", () => {
    const now = 1_000_000_000_000;
    expect(relativeTime(now - 10_000, now)).toBe("now");
    expect(relativeTime(now - 5 * 60_000, now)).toBe("5 min");
    expect(relativeTime(now - 3 * 3_600_000, now)).toBe("3 h");
    expect(relativeTime(now - 30 * 3_600_000, now)).toBe("Yesterday");
  });

  it("list keys", () => {
    expect(listTarget("ArrowDown", 0, 3)).toBe(1);
    expect(listTarget("ArrowDown", 2, 3)).toBe(2);
    expect(listTarget("ArrowUp", 0, 3)).toBe(0);
    expect(listTarget("End", 0, 3)).toBe(2);
    expect(listTarget("x", 0, 3)).toBeNull();
    expect(listTarget("ArrowDown", 0, 0)).toBeNull();
  });

  it("images: type and size are checked before reading", async () => {
    expect(await readImage(new File(["x"], "a.svg", { type: "image/svg+xml" }))).toMatch(/not a PNG/);
    const big = new File([new Uint8Array(5 * 1024 * 1024 + 1)], "big.png", { type: "image/png" });
    expect(await readImage(big)).toMatch(/larger than 5 MiB/);
  });
});
