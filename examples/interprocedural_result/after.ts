function greetingPrefix(): string {
  return "Welcome, ";
}

function makeGreeting(name: string): string {
  return greetingPrefix() + name;
}

function addPunctuation(message: string): string {
  return message + "!";
}

export function buildMessage(): string {
  const greeting = makeGreeting("Ada");
  return addPunctuation(greeting);
}
