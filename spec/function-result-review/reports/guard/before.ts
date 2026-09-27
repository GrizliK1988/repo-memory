function result(enabled, blocked, ready, approved) {
  if (!enabled) return "disabled";
  if (blocked) return "blocked";
  if (ready) return "ok";
  return "pending";
}
