// Roles, names, ARIA states and keyboard behaviour of the ui/ controls (SYSTEM-003).
// Markup comes from react-dom/server; handlers are invoked through the elements the
// (hook-free) components return, so no DOM is needed.

import type { ReactElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import {
  Button,
  Field,
  IconButton,
  keyLabels,
  Led,
  Meter,
  RateMeter,
  Segmented,
  segmentedTarget,
  Stepper,
  stepperTarget,
  Switch,
} from "../src/ui/primitives";

type El = ReactElement<Record<string, unknown> & { children?: unknown }>;
const key = (k: string) => ({ key: k, preventDefault: vi.fn(), currentTarget: { parentElement: null } });

describe("ui primitives", () => {
  it("IconButton requires a label and sets aria-pressed", () => {
    // @ts-expect-error label is mandatory
    void (() => <IconButton>x</IconButton>);
    const html = renderToStaticMarkup(
      <IconButton label="Toggle panel" pressed>
        <svg />
      </IconButton>,
    );
    expect(html).toContain('aria-label="Toggle panel"');
    expect(html).toContain('aria-pressed="true"');
    expect(renderToStaticMarkup(<IconButton label="Settings">x</IconButton>)).not.toContain("aria-pressed");
  });

  it("Button loading sets aria-busy and ignores clicks", () => {
    const onClick = vi.fn();
    const busy = Button({ loading: true, onClick, children: "Run" }) as El;
    expect(busy.props["aria-busy"]).toBe(true);
    expect(busy.props.disabled).toBeUndefined(); // still focusable
    (busy.props.onClick as (e: unknown) => void)({ preventDefault: vi.fn() });
    expect(onClick).not.toHaveBeenCalled();
    expect(renderToStaticMarkup(<Button loading>Run</Button>)).toContain('class="spinner"');
    const idle = Button({ onClick, children: "Run" }) as El;
    (idle.props.onClick as (e: unknown) => void)({});
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("Segmented arrow keys select", () => {
    expect(segmentedTarget("ArrowRight", 0, 3)).toBe(1);
    expect(segmentedTarget("ArrowLeft", 0, 3)).toBe(2);
    expect(segmentedTarget("End", 0, 3)).toBe(2);
    expect(segmentedTarget("a", 0, 3)).toBeNull();
    const onChange = vi.fn();
    const options = [{ value: "system", label: "System" }, { value: "light", label: "Light" }, { value: "dark", label: "Dark" }];
    const group = Segmented({ value: "system", options, onChange, label: "Theme" }) as El;
    const radios = (group.props.children as unknown[])[1] as El[];
    expect(radios.map((r) => r.props.tabIndex)).toEqual([0, -1, -1]);
    (radios[0]!.props.onKeyDown as (e: unknown) => void)(key("ArrowRight"));
    expect(onChange).toHaveBeenLastCalledWith("light");
    const html = renderToStaticMarkup(<Segmented value="dark" options={options} onChange={onChange} label="Theme" />);
    expect(html).toContain('role="radiogroup"');
    expect(html).toContain('aria-label="Theme"');
    expect(html.match(/aria-checked="true"/g)).toHaveLength(1);
  });

  it("Switch toggles with Space", () => {
    const onChange = vi.fn();
    const sw = Switch({ checked: false, onChange, label: "Word wrap" }) as El;
    expect(sw.props.role).toBe("switch");
    expect(sw.props["aria-checked"]).toBe(false);
    const space = key(" ");
    (sw.props.onKeyDown as (e: unknown) => void)(space);
    expect(onChange).toHaveBeenLastCalledWith(true);
    expect(space.preventDefault).toHaveBeenCalled(); // no second toggle from the native activation
    (sw.props.onKeyDown as (e: unknown) => void)(key("Enter"));
    (sw.props.onKeyDown as (e: unknown) => void)(key("a"));
    expect(onChange).toHaveBeenCalledTimes(2);
  });

  it("Stepper changes with Up/Down and clamps", () => {
    expect(stepperTarget("ArrowUp", 13, { min: 8, max: 40 })).toBe(14);
    expect(stepperTarget("ArrowDown", 8, { min: 8, max: 40 })).toBe(8);
    expect(stepperTarget("Tab", 13, { min: 8, max: 40 })).toBeNull();
    const onChange = vi.fn();
    const st = Stepper({ value: 13, min: 8, max: 40, onChange, label: "Editor font size" }) as El;
    const [dec, input, inc] = st.props.children as El[];
    (input!.props.onKeyDown as (e: unknown) => void)(key("ArrowUp"));
    expect(onChange).toHaveBeenLastCalledWith(14);
    (input!.props.onKeyDown as (e: unknown) => void)(key("ArrowDown"));
    expect(onChange).toHaveBeenLastCalledWith(12);
    expect(dec!.props["aria-label"]).toBe("Decrease Editor font size");
    expect(inc!.props["aria-label"]).toBe("Increase Editor font size");
  });

  it("Led with label has role img", () => {
    expect(renderToStaticMarkup(<Led state="run" label="Task running" />)).toBe('<span class="led" data-state="run" role="img" aria-label="Task running"></span>');
    expect(renderToStaticMarkup(<Led state="off" />)).toContain('aria-hidden="true"');
  });

  it("Keys renders platform modifiers", () => {
    expect(keyLabels("Ctrl Alt Shift R", true)).toEqual(["⌘", "⌥", "Shift", "R"]);
    expect(keyLabels("Ctrl Alt Shift R", false)).toEqual(["Ctrl", "Alt", "Shift", "R"]);
  });

  it("meters and fields expose their state", () => {
    const m = renderToStaticMarkup(<Meter value={0.42} label="Build" />);
    expect(m).toContain('role="progressbar"');
    expect(m).toContain('aria-valuenow="42"');
    expect(renderToStaticMarkup(<RateMeter levels={[1]} label="Output rate" />)).toContain('role="img" aria-label="Output rate"');
    const f = renderToStaticMarkup(<Field invalid describedBy="err" aria-label="Size" />);
    expect(f).toContain('aria-invalid="true"');
    expect(f).toContain('aria-describedby="err"');
  });
});
