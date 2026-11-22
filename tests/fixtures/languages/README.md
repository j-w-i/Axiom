# Language fixtures (M5.5-QA-001)

One small project per tier-1 language (ADR-0021). Each uses a standard-library symbol, so the
end-to-end scenario can check completion with documentation, hover and go-to-definition into the
SDK once the language server is installed: `os.path.join` (Python), `toSorted` (TypeScript),
`HashMap::new` (Rust), `strings.ToUpper` (Go), `strlen` (C), `std::sort` (C++), `List.of` (Java),
`OrderBy` (C#), `sorted` (Kotlin), `joined` (Swift), `max` (Dart), and the HTML, CSS, JSON, YAML,
TOML, Markdown, Bash and Dockerfile files their servers read.
