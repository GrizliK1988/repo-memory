function result(enabled: boolean, ready: boolean) {
  if (enabled && ready) return "ok";
  return "skip";
}
