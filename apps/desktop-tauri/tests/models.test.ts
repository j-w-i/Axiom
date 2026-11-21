// Model provider forms (M3-UI-003): the configuration a form describes, provider ids.
import { describe, expect, it } from "vitest";
import { formConfig } from "../src/components/Models";
import { providerId } from "../src/state/agent";

describe("models", () => {
  it("provider ids follow the daemon's rule", () => {
    expect(providerId("http://127.0.0.1:11434")).toBe("127-0-0-1-11434");
    expect(providerId("My Server!")).toBe("my-server");
    expect(providerId("")).toBe("provider");
    expect(providerId("x".repeat(60))).toHaveLength(40);
  });

  it("forms describe configurations", () => {
    expect(formConfig("anthropic", "", "", [])).toEqual({ id: "anthropic", kind: "anthropic", name: "Anthropic" });
    expect(formConfig("openai", "", "", [])).toEqual({ id: "openai", kind: "openaiChat", name: "OpenAI" });
    expect(formConfig("openai", "http://127.0.0.1:8000/v1", "", [])).toEqual({ id: "127-0-0-1-8000", kind: "openaiChat", name: "127.0.0.1:8000", baseUrl: "http://127.0.0.1:8000/v1" });
    expect(formConfig("openai", "example.com", "", [])).toMatch(/starts with https/);
    expect(formConfig("openai", "https://api.example.ai/v1", "Example", [])).toMatchObject({ id: "api-example-ai", name: "Example" });
  });
});
