# K017: pinned Bluesky web metadata

Exact source files from social-app revision
`7bb3a0a14b947c77676824ba5faf99ff93d6c976`. `manifest.json` records URLs,
Git blob identities and SHA-256 hashes. The upstream MIT license is in LICENSE.
This deliberately partial repository slice keeps unavailable internal modules
visible as boundaries; it does not claim whole-repository coverage or K025 flow
acceptance. No application scripts execute and the test accesses no network.

The selected config is the original `tsconfig.check.web.json`, extending the
original root config. The supplied React Native base config is version 0.81.6,
the exact version in the pinned revision's pnpm lockfile. This declaration/config
package is MIT licensed (React Native contributors). It is supplied explicitly,
not resolved from the developer's node_modules. All original source bytes remain
unchanged, including tsconfig comments.

Independently reviewed expectations:

- Both `getContentLanguages()` calls and `getAppLanguageAsContentLanguage()` in
  `src/lib/api/feed/custom.ts` resolve through `#/state/preferences/languages`
  to their function bodies in `src/state/preferences/languages.tsx`.
- Both `isXrpcError()` calls in `src/lib/strings/errors.ts` resolve through
  `#/lib/xrpc-error` to the exported body in `src/lib/xrpc-error.ts`, and bind to
  separate call operations in the existing scalar IR.
- External `getMain()` from `@atproto/lex` remains an external-package call with
  unknown behavior. Missing `#/lexicons` remains an internal missing boundary.
- The effective module suffixes are `.web`, then the unsuffixed file. Metadata
  does not claim that async functions, class methods, heap operations or external
  behavior are already supported by the scalar solver.
