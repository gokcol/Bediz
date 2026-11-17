# Bediz 3.1.0 — build 125

After placing a bucket fill, choosing another color, opacity or transparency now configures the next fill. Completed fills retain their own colors. Selecting an existing fill with Select or Layers still allows intentional recoloring and region updates; the inspector identifies this state as **Selected fill**.

New fills finish without an annotation selection. Explicit **Update fill region** operations retain their selected fill. Rectangle, lasso and pixel-mask limits remain available for the next fill. The Bediz branding and upgrade identities from build 124 are unchanged.

## Local fix validation

The evidence in `dist/fill-color-build125/validation.json` records the local Mac fix build before public release preparation:

- 19 fill tests passed; one fixture test was ignored. Eight tool-preference tests and four update-metadata tests passed. Formatting and Clippy passed.
- Regression tests click the actual quick-color controls. They verify that a completed red fill stays red when blue is chosen, the next region becomes blue, and choosing transparency does not alter either completed fill.
- Tests cover independent rendered pixels, no extra document undo entry for choosing the next color, undo/redo, saved-project reopening, intentional recoloring, explicit region updates and creating a new fill after editing an existing one.
- The Mac app was Developer ID signed with hardened runtime and a timestamp. Apple accepted notarization `d1e6bb45-4c4e-43b5-99fa-65c7641b0405`; its stapled ticket and Gatekeeper assessment passed. All app files in the ZIP matched the verified bundle.
- The local build was installed as Bediz 3.1.0 build 125. The installed binary matched the verified bundle. Its version and next-fill help were observed, and the previous saved project reopened. User artwork was not edited during that UI check.

The local evidence records source base `d15ede74742ba12f2ccb5b616a6f4f59acd6de45` plus working patch SHA-256 `f6fc0a827ba1aa86fe90a0e2398949dd9517a52a3c12a07ae563d3984204ff33`. It does not establish the identity or validation of the final public release artifacts, which must include the updated Store metadata.

## Verified release artifacts

The published release is [`v3.1.0-build125`](https://github.com/gokcol/MyImage/releases/tag/v3.1.0-build125), app version 3.1.0 build 125, with Microsoft Store package version **3.1.3.0**. The existing Store product, package identity, Mac bundle identifier, executable, storage paths and `.myimage` project format are retained.

| Release check | Status |
| --- | --- |
| Final release commit and tag | `167b420e6cf24c73c81708344ba065c8a849dc32`; `v3.1.0-build125` pushed |
| Final native checks and Windows CI | [Native checks](https://github.com/gokcol/MyImage/actions/runs/44094440043) passed: 719 app + 19 packaging tests; 27 ignored. [Disposable Windows MSIX install/removal](https://github.com/gokcol/MyImage/actions/runs/44094440088) passed |
| Rebuilt Mac app signing and notarization | Accepted: `ec78f0fe-66d8-4a93-a8b9-43ea0f2ad90d` |
| Mac ZIP/DMG contents, tickets, Gatekeeper and hashes | Passed; DMG accepted: `f662a901-250e-4118-a217-b490ec0b3ed4` |
| Windows Setup and portable ZIP packaging/validation | [Windows distribution workflow](https://github.com/gokcol/MyImage/actions/runs/44094628226) passed; Bediz metadata, embedded release notes, executable architecture and SHA-256 checks passed |
| Store MSIX 3.1.3.0 packaging and validation | [Store packaging workflow](https://github.com/gokcol/MyImage/actions/runs/44094655636) passed; Bediz display names, stable package identity, x64 executable and embedded release notes verified. Only `runFullTrust` is restricted; startup is disabled by default |
| GitHub assets and published-hash verification | Published as the latest release; all nine distribution assets plus the validation record match their verified local SHA-256 digests |
| Microsoft Store upload and submission | Uploaded, validated and submitted as submission 7 (`1152921505702428856`); status **In certification / Pre-processing** on 17 November 2026 |
| Microsoft Store approval/publication | Pending Microsoft approval; automatic publication after certification |

Mac release packages were rebuilt from the final tagged source with Store metadata 3.1.3.0. The earlier installed local fix build is functionally the same fix but retains the earlier Store metadata; it is not the final public artifact. The final Mac binary SHA-256 is `edf02c7da0c108a4661051e196cd6b541832ef69e84be7c4588ab5ac13ad0307`. All app files in the ZIP match the verified bundle; the read-only DMG contains the same executable. Both packages passed signature, stapled-ticket and Gatekeeper checks.

The published `release-validation.json` records source identity, all nine distribution asset hashes, workflow results and the separate Store package checks. Store MSIX SHA-256: `b5d1fa77d9f402c138742f45e42103ef83dec114a87d5d3ea3d816f34cd33b5e`. GitHub publication and Store certification are separate from these completed artifact checks.

Interactive Windows capture, clipboard and device testing remain pending. Hosted packaging and installation checks do not substitute for those native interaction checks. The prior build 124 release and Store submission record is preserved in [Bediz release and migration details](BEDIZ.md).
