import { greetingPrefix } from "./prefix";

export function makeGreeting(name: string): string {
  return greetingPrefix() + name;
}
