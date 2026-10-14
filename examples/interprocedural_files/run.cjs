const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const ts = require("../../tools/typescript-metadata/node_modules/typescript");

for (const side of ["before", "after"]) {
  const cache = new Map();
  function load(filename) {
    if (cache.has(filename)) return cache.get(filename).exports;
    const source = fs.readFileSync(filename, "utf8");
    const { outputText } = ts.transpileModule(source, {
      compilerOptions: {
        module: ts.ModuleKind.CommonJS,
        target: ts.ScriptTarget.ES2022,
      },
    });
    const module = { exports: {} };
    cache.set(filename, module);
    vm.runInNewContext(outputText, {
      exports: module.exports,
      module,
      require: (specifier) => {
        if (!specifier.startsWith("./")) throw new Error("Only local fixture imports are supported");
        return load(path.resolve(path.dirname(filename), `${specifier}.ts`));
      },
    }, { filename });
    return module.exports;
  }
  const result = load(path.join(__dirname, side, "main.ts")).buildMessage();
  console.log(`${side}: ${JSON.stringify(result)}`);
}
