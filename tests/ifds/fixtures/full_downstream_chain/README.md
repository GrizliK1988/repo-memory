# Full downstream chain

Reviewed stage-0 fixture: the changed selected value must continue through the
intermediate calculation to the return and record a caller-scope boundary. The
containing-function analysis has complete downstream coverage inside its declared
scope, but the returned value's lifecycle remains open. Reporting only the first
consumer is incomplete.
