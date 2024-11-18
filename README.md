# expr-changelog-generation

Experiment with CHANGELOG.md generation

Created by working with ChatGPT4o:
https://chatgpt.com/share/673bc316-af80-800c-94ba-0719a1311d10

## Compiling

Initial compile of second code suggestion, the first one it reported
there was syntax errors and so this is the output from compiler from
this second version which is this commit. Not to bad certainly better
than I could have done:
```
wink@3900x 24-11-18T22:45:40.051Z:~/prgs/rust/myrepos/expr-changelog-generation (main)
$ cargo build
  Downloaded serde_derive v1.0.215
   ..
   Compiling git2 v0.19.0
   Compiling expr-changelog-generation v0.1.0 (/home/wink/prgs/rust/myrepos/expr-changelog-generation)
warning: unused import: `Utc`
 --> src/main.rs:2:29
  |
2 | use chrono::{NaiveDateTime, Utc};
  |                             ^^^
  |
  = note: `#[warn(unused_imports)]` on by default

warning: use of deprecated associated function `chrono::NaiveDateTime::from_timestamp_opt`: use `DateTime::from_timestamp` instead
  --> src/main.rs:38:32
   |
38 |     let naive = NaiveDateTime::from_timestamp_opt(timestamp, 0).unwrap();
   |                                ^^^^^^^^^^^^^^^^^^
   |
   = note: `#[warn(deprecated)]` on by default

warning: value assigned to `current_tag` is never read
  --> src/main.rs:49:13
   |
49 |     let mut current_tag = "[unreleased]".to_string();
   |             ^^^^^^^^^^^
   |
   = help: maybe it is overwritten before being read?
   = note: `#[warn(unused_assignments)]` on by default

warning: unused `Result` that must be used
  --> src/main.rs:24:5
   |
24 |     revwalk.set_sorting(git2::Sort::TIME);
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = note: this `Result` may be an `Err` variant, which should be handled
   = note: `#[warn(unused_must_use)]` on by default
help: use `let _ = ...` to ignore the resulting value
   |
24 |     let _ = revwalk.set_sorting(git2::Sort::TIME);
   |     +++++++

warning: `expr-changelog-generation` (bin "expr-changelog-generation") generated 4 warnings (run `cargo fix --bin "expr-changelog-generation"` to apply 1 suggestion)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 17.86s
wink@3900x 24-11-18T22:46:05.351Z:~/prgs/rust/myrepos/expr-changelog-generation (main)
```


## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
