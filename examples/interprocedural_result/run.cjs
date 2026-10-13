const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const ts = require("../../tools/typescript-metadata/node_modules/typescript");

for (const side of ["before", "after"]) {
  const source = fs.readFileSync(path.join(__dirname, `${side}.ts`), "utf8");
  const { outputText } = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2022,
    },
  });
  const context = { exports: {} };
  vm.runInNewContext(outputText, context);
  console.log(`${side}: ${JSON.stringify(context.exports.buildMessage())}`);
}
