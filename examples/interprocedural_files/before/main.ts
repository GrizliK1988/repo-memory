import { makeGreeting } from "./greeting";
import { addPunctuation } from "./punctuation";

export function buildMessage(): string {
  const greeting = makeGreeting("Ada");
  return addPunctuation(greeting);
}
