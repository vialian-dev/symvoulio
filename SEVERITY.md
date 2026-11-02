# Severity definitions for independent review

Symvoulio's own code was reviewed by an independent reviewer from a different AI provider. Each finding was rated on four levels. These written definitions were applied from review pass 7 onward; passes 1–6 used the same four-level scale without written definitions.

- **Blocker.** A defect that makes a core promise of the product false, or that causes data loss or corruption, leakage of credentials or private content, a break of provider isolation, or inability to complete the main workflow (author → independent review → gate → human decision → export). A release does not ship with a Blocker.
- **High.** A documented feature that fails or gives a wrong result in a realistic scenario with no reasonable workaround, or that materially misleads the user about what happened. It is fixed before release.
- **Medium.** Wrong behaviour that has a workaround or occurs only in edge cases; reliability or robustness gaps that do not corrupt results.
- **Low.** Cosmetic, wording or test-quality issues with no effect on results.
