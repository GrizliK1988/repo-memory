# Bluesky social-app PR #11693 fixture

Source: https://github.com/bluesky-social/social-app/pull/11693/files
File: `modules/bottom-sheet/src/BottomSheetNativeComponent.tsx`

- Original revision: `277f4637de64090bcbd68fef3451d967b1b86ea9`
- Modified revision: `d8b17bc75647722536e5fd4b0fb3095b983e2f05`
- Original Git blob prefix: `f0306336678`
- Modified Git blob: `0d5df0a6b6f09271a490c2df2329c4a38a1a5f3a`

The before/after files preserve the complete upstream source and line numbers.
`change.diff` is the original single-file Git diff, not regenerated during tests.
The fixture is embedded with `include_str!`; tests require no network access.

The Android-only addition of a comment and `flexShrink: 1` changes exactly one
declaration: `BottomSheetNativeComponentInner` (Function). Its range changes from
lines 125–199 to 125–204. The outer class and all local declarations are unchanged.
The object-literal style property is not a separate declaration. The function's
`child_changes` identifies it as an added `style[2].flexShrink` property on
`View` (`NativeView[0]/View[0]`), with value `1` at modified line 189.

The upstream MIT license is reproduced in LICENSE and applies to these fixtures.
