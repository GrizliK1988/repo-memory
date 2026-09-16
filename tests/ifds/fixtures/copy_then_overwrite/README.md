# Copy then overwrite

Reviewed stage-0 fixture: copying `x` to `y` preserves the original value's path to
the return even when `x` is overwritten afterward. Both reviewed relations are
retained rather than additions or removals.
