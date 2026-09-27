function status(enabled: boolean, blocked: boolean, ready: boolean, approved: boolean) {
  if (!enabled) return "disabled";
  if (blocked) return "blocked";
  if (ready && approved) return "ok";
  return "pending";
}
