## Verdict: PASS_WITH_NOTES

1. RESOLVED: Groq `build_plan_payload` now sets `strict` only for `gpt-oss` (`src/provider/groq.rs:110-126`), and the qwen regression test explicitly checks `qwen => strict:false` / `gpt-oss => strict:true` (`src/provider/groq.rs:176-183`).
2. RESOLVED: `DiffBudget` now makes the contract explicit that `total_bytes` applies on every path and `per_file_bytes` is grouping-only (`src/diff.rs:39-49`), and the code matches that in `gather`, `gather_for_files`, and `gather_for_grouping` (`src/diff.rs:119-167`).
3. RESOLVED: `Provider::name()` is restored on the trait (`src/provider/mod.rs:27-29`), implemented in Groq/OpenAI/Google (`src/provider/groq.rs:53-55`, `src/provider/openai.rs:58-60`, `src/provider/gemini.rs:70-72`), and used in `src/main.rs:75-79`.

NEW findings: None from source review of `git diff main...HEAD`; note that I could not run `cargo test` in this environment because the read-only sandbox blocks Cargo from opening `target/debug/.cargo-lock`, so the regression check is static rather than runtime-validated.