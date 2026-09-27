function result(enabled, blocked, ready, approved) {
  if (!enabled) return "disabled";
  if (blocked) return "blocked";
  if (ready && approved) return "ok";
  return "pending";
}
