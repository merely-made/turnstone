# Upstream freshness audit, 2026-10-07

Observed at 2026-10-07T09:19:06.523827+00:00. This is source research. The consumer remains on
Servo 0.7.0 at `aac43a3f31a259f04a574f5ec4e959c943ad7cc7`; the running build inputs were not changed.

| Upstream checkpoint | Exact source and date | Published wrapper / SpiderMonkey binding |
| --- | --- | --- |
| Latest stable Servo 0.7.0 | [`aac43a3f31a259f04a574f5ec4e959c943ad7cc7`](https://github.com/servo/servo/tree/aac43a3f31a259f04a574f5ec4e959c943ad7cc7), released 2026-10-05T18:32:02Z | `mozjs =0.26.4` / `mozjs_sys =153.3.0-1` |
| Observed Servo main | [`3058b7ab31a81958260dbce31abae4aad77fd03d`](https://github.com/servo/servo/commit/3058b7ab31a81958260dbce31abae4aad77fd03d), 2026-10-07T08:26:22Z | `mozjs =0.26.7` / `mozjs_sys =153.3.0-3` |
| Published mozjs security update | [`34ad101f42bafbb197cb27b4b0d343469f7799e7`](https://github.com/servo/mozjs/commit/34ad101f42bafbb197cb27b4b0d343469f7799e7), 2026-10-07T05:52:48Z | `mozjs 0.26.8` / `mozjs_sys =153.4.0-0` |

The [Servo release API](https://api.github.com/repos/servo/servo/releases/latest)
and retained tag response establish the released checkpoint. The immutable
[release manifest](https://github.com/servo/servo/blob/aac43a3f31a259f04a574f5ec4e959c943ad7cc7/Cargo.toml) and
[observed main manifest](https://github.com/servo/servo/blob/3058b7ab31a81958260dbce31abae4aad77fd03d/Cargo.toml)
show the two wrapper selections. The three retained published wrapper bundles
were checked against their registry checksums; their normalized manifests bind
the exact `mozjs_sys` versions above.

The official commit calls the October 7 change “Security bump SpiderMonkey to
153.4.0 (#829)” and records Mozilla source changeset
`ec9c1cc8a5cb6fac9d4d17e4f141fb42a8089b4c`. Registry metadata records
[mozjs 0.26.8](https://crates.io/api/v1/crates/mozjs/0.26.8) published
2026-10-07T07:09:35.375195Z and
[mozjs_sys 153.4.0-0](https://crates.io/api/v1/crates/mozjs_sys/153.4.0-0)
published 2026-10-07T07:08:59.853147Z; neither was yanked
at observation. This update includes engine and private ICU Rust source changes.
Its [sys manifest](https://github.com/servo/mozjs/blob/34ad101f42bafbb197cb27b4b0d343469f7799e7/mozjs-sys/Cargo.toml)
continues to select `icu_capi =2.1.2`.

The latest stable Servo release therefore does not include this newer security
package, and the observed Servo main has not adopted it either. This records a
freshness limit of the selected release. It does not assess vulnerability
severity, establish 153.4 compatibility, or change the selected AAC checkpoint.
The current ICU source-isolation and manifest-compatibility proofs retain their
own scope; the new security commit is not an interchangeable manifest-only fork.

[upstream-freshness-20261007-summary.json](upstream-freshness-20261007-summary.json) contains source rows, package
checksums, timestamps, cautions and the byte/hash index for the retained evidence.
The [evidence manifest](upstream-freshness-20261007-evidence-manifest.json) also binds this note
and summary. Live `latest`/`main` endpoints are preserved observations; the full
commit links above remain immutable source references.
