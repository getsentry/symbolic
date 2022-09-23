import { namedFn } from "./sync.mjs";
import { asyncNamedFn } from "./async.mjs";

// node has a default limit of 10
Error.stackTraceLimit = Infinity;

let output = "# sync stack trace\n";

try {
  namedFn();
} catch (e) {
  output += e.stack;
}

output += "\n\n";

output += "# async stack trace\n";

try {
  await asyncNamedFn();
} catch (e) {
  output += e.stack;
}

output += "\n";

console.log(output);
