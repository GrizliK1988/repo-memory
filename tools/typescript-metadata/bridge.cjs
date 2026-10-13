"use strict";

// Only this bridge and its pinned compiler execute. Project files are virtual data.
const fs = require("node:fs");
const path = require("node:path").posix;
const compilerPath = require("node:path").join(__dirname, "node_modules/typescript/lib/typescript.js");
const ts = require(compilerPath);
const PROTOCOL = 1;
const COMPILER = "5.9.3";
const ROOT = "/snapshot";
const LIB = "/__compiler";
const compare = (a, b) => a < b ? -1 : a > b ? 1 : 0;

function analyze(request) {
  const fail = (code, message, requestedBy = null, specifier = null, candidates = []) => {
    throw { code, snapshot: request.snapshot, message, requested_by: requestedBy, specifier, candidates };
  };
  if (request.protocol_version !== PROTOCOL || request.compiler_version !== COMPILER || ts.version !== COMPILER) {
    fail("unsupported_version", `Expected protocol ${PROTOCOL} and TypeScript ${COMPILER}; got compiler ${ts.version}`);
  }
  const files = new Map();
  for (const [name, file] of Object.entries(request.files)) files.set(`${ROOT}/${name}`, file.text);
  for (const [name, file] of Object.entries(request.options.supplied_files)) {
    if (files.has(`${ROOT}/${name}`)) fail("invalid_request", "Supplied files cannot replace snapshot files");
    files.set(`${ROOT}/${name}`, file.text);
  }
  // Compiler standard-library declarations come only from the pinned tool package.
  const libraryDirectory = require("node:path").dirname(compilerPath);
  for (const name of fs.readdirSync(libraryDirectory).filter(name => /^lib\..*\.d\.ts$/.test(name))) {
    files.set(`${LIB}/${name}`, fs.readFileSync(require("node:path").join(libraryDirectory, name), "utf8"));
  }
  const relative = name => name.startsWith(`${ROOT}/`) ? name.slice(ROOT.length + 1) : null;
  const configurations = new Map();
  const readFile = name => {
    name = path.normalize(name);
    const text = files.get(name);
    if (text !== undefined && name.endsWith(".json") && relative(name)) configurations.set(relative(name), text);
    return text;
  };
  const fileExists = name => files.has(path.normalize(name));
  const directories = new Set([ROOT, LIB, "/"]);
  for (const name of files.keys()) {
    let directory = path.dirname(name);
    while (!directories.has(directory)) {
      directories.add(directory);
      directory = path.dirname(directory);
    }
  }
  const entries = directory => {
    directory = path.normalize(directory);
    return {
      files: [...files.keys()].filter(name => path.dirname(name) === directory).map(name => path.basename(name)).sort(),
      directories: [...directories].filter(name => name !== directory && path.dirname(name) === directory).map(name => path.basename(name)).sort(),
    };
  };
  const configHost = {
    useCaseSensitiveFileNames: true, readFile, fileExists,
    directoryExists: name => directories.has(path.normalize(name)),
    getCurrentDirectory: () => ROOT,
    realpath: name => path.normalize(name),
    readDirectory: (root, extensions, excludes, includes, depth) => ts.matchFiles(root, extensions, excludes, includes, true, ROOT, depth, entries, name => name),
    onUnRecoverableConfigFileDiagnostic: diagnostic => fail("invalid_config", ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n")),
  };

  // A preflight makes missing extends machine-readable instead of defaulting options.
  const configVisited = new Set();
  function checkConfiguration(name, stack = new Set()) {
    name = path.normalize(name);
    if (stack.has(name)) fail("invalid_config", "Cyclic tsconfig extends", relative(name));
    if (configVisited.has(name)) return;
    const loaded = ts.readConfigFile(name, readFile);
    if (loaded.error) fail("invalid_config", ts.flattenDiagnosticMessageText(loaded.error.messageText, "\n"), relative(name));
    const nextStack = new Set([...stack, name]);
    const inherited = loaded.config.extends === undefined ? [] : (Array.isArray(loaded.config.extends) ? loaded.config.extends : [loaded.config.extends]);
    for (const specifier of inherited) {
      if (typeof specifier !== "string") fail("invalid_config", "extends must contain config paths", relative(name));
      if (specifier.startsWith("/")) fail("invalid_config", "Absolute extends paths are outside the snapshot", relative(name), specifier);
      let resolved;
      if (specifier.startsWith(".")) {
        const candidate = path.resolve(path.dirname(name), specifier);
        resolved = [candidate, `${candidate}.json`, `${candidate}/tsconfig.json`].find(fileExists);
      } else {
        let directory = path.dirname(name);
        while (directory === ROOT || directory.startsWith(`${ROOT}/`)) {
          const candidate = `${directory}/node_modules/${specifier}`;
          resolved = [candidate, `${candidate}.json`, `${candidate}/tsconfig.json`].find(fileExists);
          if (!resolved) {
            const manifestPath = `${candidate}/package.json`;
            const manifestText = readFile(manifestPath);
            if (manifestText) {
              let manifest;
              try { manifest = JSON.parse(manifestText); } catch { fail("invalid_config", "Invalid config package.json", relative(manifestPath)); }
              const target = path.resolve(candidate, manifest.tsconfig || "tsconfig.json");
              if (fileExists(target)) resolved = target;
            }
          }
          if (resolved) break;
          directory = path.dirname(directory);
        }
      }
      if (!resolved || !relative(resolved)) fail("missing_base_config", `Provide the base config ${specifier} requested by ${relative(name)}`, relative(name), specifier);
      checkConfiguration(resolved, nextStack);
    }
    for (const reference of loaded.config.references || []) {
      if (typeof reference.path !== "string" || reference.path.startsWith("/")) fail("invalid_config", "Project references must stay inside the virtual snapshot", relative(name));
      const candidate = path.resolve(path.dirname(name), reference.path);
      const resolved = candidate.endsWith(".json") ? candidate : `${candidate}/tsconfig.json`;
      if (!relative(resolved) || !fileExists(resolved)) fail("missing_tsconfig", "Provide the referenced project's tsconfig", relative(name), reference.path);
      checkConfiguration(resolved, nextStack);
    }
    configVisited.add(name);
  }
  function parseConfiguration(name) {
    checkConfiguration(name);
    const parsed = ts.getParsedCommandLineOfConfigFile(name, { noEmit: true }, configHost);
    const errors = parsed.errors.filter(d => d.code !== 18002 && d.code !== 18003);
    if (errors.length) fail("invalid_config", errors.map(d => ts.flattenDiagnosticMessageText(d.messageText, "\n")).join("\n"), relative(name));
    return parsed;
  }
  let configPath = request.options.tsconfig;
  let parsed;
  if (configPath) {
    if (!fileExists(`${ROOT}/${configPath}`)) fail("missing_tsconfig", "Provide the selected tsconfig", null, configPath);
    parsed = parseConfiguration(`${ROOT}/${configPath}`);
  } else {
    const candidates = [...files.keys()].filter(name => relative(name) && !name.includes("/node_modules/") && /^tsconfig(?:\..*)?\.json$/.test(path.basename(name))).sort();
    const compatible = [];
    const failures = [];
    for (const candidate of candidates) {
      try {
        const candidateConfig = parseConfiguration(candidate);
        if (request.selected_paths.every(name => candidateConfig.fileNames.includes(`${ROOT}/${name}`))) compatible.push([candidate, candidateConfig]);
      } catch (error) { failures.push(error); }
    }
    if (compatible.length > 1) fail("ambiguous_tsconfig", "Select a tsconfig explicitly", null, null, compatible.map(([name]) => relative(name)));
    if (!compatible.length) {
      if (failures.length === 1) throw failures[0];
      fail("missing_tsconfig", "Provide a tsconfig covering the selected files", null, null, candidates.map(relative));
    }
    [configPath, parsed] = compatible[0];
    configPath = relative(configPath);
    // Record only the selected config's dependencies, not attempted alternatives.
    configurations.clear(); configVisited.clear();
    parsed = parseConfiguration(`${ROOT}/${configPath}`);
  }
  const options = parsed.options;
  // The config is authoritative. K017 records its environment; it never runs
  // plugins, package scripts, emit, or semantic type-check acceptance gates.
  const host = {
    ...configHost,
    useCaseSensitiveFileNames: () => true,
    getSourceFile: (name, languageVersion) => {
      const text = readFile(name);
      return text === undefined ? undefined : ts.createSourceFile(name, text, languageVersion, true);
    },
    getDefaultLibFileName: compilerOptions => `${LIB}/${ts.getDefaultLibFileName(compilerOptions)}`,
    getDefaultLibLocation: () => LIB,
    writeFile: () => { throw new Error("Project emit is forbidden"); },
    getCanonicalFileName: name => name,
    getNewLine: () => "\n",
    getParsedCommandLine: name => parseConfiguration(name),
  };
  const program = ts.createProgram({ rootNames: [...new Set([...parsed.fileNames, ...request.selected_paths.map(name => `${ROOT}/${name}`)])].sort(), options, host, projectReferences: parsed.projectReferences });
  const checker = program.getTypeChecker();
  const cache = ts.createModuleResolutionCache(ROOT, name => name, options);
  const excluded = name => request.options.excluded_paths.some(prefix => name === prefix || name.startsWith(`${prefix}/`));
  const sourceFiles = program.getSourceFiles().filter(file => relative(file.fileName) && !file.fileName.includes("/node_modules/") && !excluded(relative(file.fileName))).sort((a, b) => compare(a.fileName, b.fileName));
  const writtenSymbols = new Set();
  function writeTarget(node) {
    if (ts.isIdentifier(node)) writtenSymbols.add(checker.getSymbolAtLocation(node));
    else if (ts.isArrayLiteralExpression(node)) node.elements.forEach(writeTarget);
    else if (ts.isObjectLiteralExpression(node)) node.properties.forEach(property => {
      if (ts.isShorthandPropertyAssignment(property)) writeTarget(property.name);
      else if (ts.isPropertyAssignment(property)) writeTarget(property.initializer);
      else if (ts.isSpreadAssignment(property)) writeTarget(property.expression);
    });
    else if (ts.isSpreadElement(node) || ts.isParenthesizedExpression(node)) writeTarget(node.expression);
  }
  function findWrites(node) {
    if (ts.isBinaryExpression(node) && node.operatorToken.kind >= ts.SyntaxKind.FirstAssignment && node.operatorToken.kind <= ts.SyntaxKind.LastAssignment) writeTarget(node.left);
    ts.forEachChild(node, findWrites);
  }
  for (const source of program.getSourceFiles()) if (relative(source.fileName)) findWrites(source);

  // One linear UTF-16 -> UTF-8 map per source, including astral code points.
  const byteOffsets = new Map();
  const lineStarts = new Map();
  function span(node) {
    const source = node.getSourceFile();
    if (!relative(source.fileName)) return null;
    let offsets = byteOffsets.get(source.fileName);
    if (!offsets) {
      offsets = new Uint32Array(source.text.length + 1);
      const starts = [0];
      let bytes = 0;
      for (let i = 0; i < source.text.length;) {
        const code = source.text.codePointAt(i);
        const width = code > 0xffff ? 2 : 1;
        offsets[i] = bytes;
        if (width === 2) offsets[i + 1] = bytes;
        bytes += Buffer.byteLength(String.fromCodePoint(code));
        i += width; offsets[i] = bytes;
        if (code === 10) starts.push(i);
      }
      byteOffsets.set(source.fileName, offsets);
      lineStarts.set(source.fileName, starts);
    }
    // SourceSpan and Tree-sitter count LF lines, whereas TS also counts CR and
    // Unicode separators. Keep line evidence consistent with the normalized IR.
    const starts = lineStarts.get(source.fileName);
    const lineAt = position => {
      let low = 0, high = starts.length;
      while (low < high) {
        const middle = (low + high) >>> 1;
        if (starts[middle] <= position) low = middle + 1;
        else high = middle;
      }
      return low;
    };
    const start = node.getStart(source);
    const end = node.getEnd();
    return { path: relative(source.fileName), byte_start: offsets[start], byte_end: offsets[end], start_line: lineAt(start), end_line: lineAt(Math.max(start, end - 1)) };
  }
  const declarations = new Map();
  function register(node) {
    const location = span(node);
    if (!location) return null;
    const kind = ts.SyntaxKind[node.kind];
    const id = `${location.path}:${location.byte_start}:${location.byte_end}:${kind}`;
    const functionValue = ts.isVariableDeclaration(node) ? node.initializer : ts.isExportAssignment(node) ? node.expression : null;
    const implementation = node.getSourceFile().isDeclarationFile ? null : node.body ? node : functionValue && (ts.isArrowFunction(functionValue) || ts.isFunctionExpression(functionValue)) ? functionValue : null;
    if (!declarations.has(id)) declarations.set(id, {
      id, name: node.name ? node.name.getText() : (ts.isExportAssignment(node) ? "default" : "<anonymous>"), kind, span: location,
      has_body: !!implementation, implementation_span: implementation ? span(implementation) : null,
      declaration_only: node.getSourceFile().isDeclarationFile,
      excluded: excluded(location.path),
    });
    return id;
  }
  const moduleResults = new Map();
  function moduleInfo(container) {
    const literal = container.moduleSpecifier;
    if (!literal || !ts.isStringLiteralLike(literal)) return null;
    const key = `${container.getSourceFile().fileName}:${container.pos}`;
    if (moduleResults.has(key)) return moduleResults.get(key);
    const specifier = literal.text;
    const resolution = ts.resolveModuleName(specifier, container.getSourceFile().fileName, options, host, cache, undefined, ts.getModeForUsageLocation(container.getSourceFile(), literal, options)).resolvedModule;
    const name = resolution && relative(resolution.resolvedFileName);
    const isAlias = Object.keys(options.paths || {}).some(pattern => matchPattern(pattern, specifier) !== undefined);
    const external = (!specifier.startsWith(".") && !isAlias && !name) || !!resolution?.isExternalLibraryImport || !!name?.includes("node_modules/");
    let status = external ? "external_package" : !name ? "missing_dependency" : excluded(name) ? "excluded_body" : /\.d\.(?:ts|mts|cts)$/.test(name) ? "declaration_only" : "resolved_body";
    // Explicit runtime aliases must agree with the compiler's selected file.
    const runtimeAlias = findMapping(request.options.runtime_aliases, specifier);
    if (runtimeAlias !== undefined) {
      const runtimeTarget = ts.resolveModuleName(`${ROOT}/${runtimeAlias}`, container.getSourceFile().fileName, options, host).resolvedModule;
      if ((runtimeTarget?.resolvedFileName || null) !== (resolution?.resolvedFileName || null)) status = "runtime_mismatch";
    }
    if (request.options.platform === "node" && options.moduleResolution === ts.ModuleResolutionKind.Bundler && status === "resolved_body") status = "runtime_mismatch";
    const info = { specifier, resolved_path: name || null, status, external_package: external ? packageName(specifier) : null };
    moduleResults.set(key, info);
    return info;
  }
  function importContainer(node) {
    let current = node;
    while (current && !ts.isSourceFile(current)) {
      if ((ts.isImportDeclaration(current) || ts.isExportDeclaration(current)) && current.moduleSpecifier) return current;
      current = current.parent;
    }
    return null;
  }
  const imports = [];
  const calls = [];
  const diagnostics = [];
  function visit(node) {
    if (ts.isFunctionDeclaration(node) || ts.isVariableDeclaration(node) || ts.isMethodDeclaration(node)) register(node);
    if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) {
      const info = moduleInfo(node);
      if (info) imports.push({ span: span(node), specifier: info.specifier, resolved_path: info.resolved_path, status: info.status, type_only: !!node.isTypeOnly || !!node.importClause?.isTypeOnly });
    }
    if (ts.isCallExpression(node) || ts.isNewExpression(node)) calls.push(resolveCall(node));
    ts.forEachChild(node, visit);
  }
  function resolveCall(node) {
    const expression = node.expression;
    const signature = checker.getResolvedSignature(node);
    const signatureInfo = signature ? {
      parameters: signature.getParameters().map(parameter => parameter.getName()),
      return_type: checker.typeToString(signature.getReturnType()),
      declaration: signature.declaration ? span(signature.declaration) : null,
    } : null;
    const chain = [];
    const seen = new Set();
    const moduleStatuses = [];
    let externalPackage = null;
    let typeOnly = false;
    const terminal = [];
    function trace(symbol) {
      if (!symbol || seen.has(symbol)) return;
      seen.add(symbol);
      for (const declaration of symbol.declarations || []) {
        const container = importContainer(declaration);
        if (container) {
          const info = moduleInfo(container);
          typeOnly ||= !!container.isTypeOnly || !!container.importClause?.isTypeOnly || !!declaration.isTypeOnly;
          moduleStatuses.push(info.status);
          externalPackage ||= info.external_package;
          chain.push({ span: span(declaration), kind: ts.isImportDeclaration(container) ? "import" : "reexport", specifier: info.specifier, resolved_path: info.resolved_path });
          if (ts.isImportSpecifier(declaration) || ts.isExportSpecifier(declaration)) {
            const importedName = declaration.propertyName || declaration.name;
            const moduleSymbol = checker.getSymbolAtLocation(container.moduleSpecifier);
            const exported = moduleSymbol && checker.getExportsOfModule(moduleSymbol).find(item => item.getName() === importedName.text);
            if (exported && exported !== symbol) trace(exported);
          }
          if (ts.isImportClause(declaration) && declaration.name) {
            const moduleSymbol = checker.getSymbolAtLocation(container.moduleSpecifier);
            const exported = moduleSymbol && checker.getExportsOfModule(moduleSymbol).find(item => item.getName() === "default");
            if (exported && exported !== symbol) trace(exported);
          }
        } else if (!ts.isSourceFile(declaration)) {
          const id = register(declaration);
          if (id) terminal.push([id, declaration]);
        }
      }
      if (symbol.flags & ts.SymbolFlags.Alias) {
        const target = checker.getAliasedSymbol(symbol);
        if (target && target !== symbol) trace(target);
      }
    }
    // For namespace access, preserve the namespace import's resolution evidence.
    if (ts.isPropertyAccessExpression(expression)) trace(checker.getSymbolAtLocation(expression.expression));
    trace(checker.getSymbolAtLocation(expression));
    // Wildcard re-export steps have no named declaration. Find a concrete route
    // from the imported module to the terminal declaration using module exports.
    const targetPaths = new Set(terminal.map(([id]) => declarations.get(id).span.path));
    const routeSeen = new Set();
    function route(name, exportName) {
      if (!name || routeSeen.has(name) || targetPaths.has(name)) return targetPaths.has(name);
      routeSeen.add(name);
      const source = program.getSourceFile(`${ROOT}/${name}`);
      if (!source) return false;
      for (const statement of source.statements) {
        if (!ts.isExportDeclaration(statement) || !statement.moduleSpecifier) continue;
        const info = moduleInfo(statement);
        if (statement.exportClause && ts.isNamedExports(statement.exportClause) && !statement.exportClause.elements.some(element => element.name.text === exportName)) continue;
        const symbol = checker.getSymbolAtLocation(statement.moduleSpecifier);
        if (!symbol || !checker.getExportsOfModule(symbol).some(item => item.getName() === exportName)) continue;
        const before = chain.length;
        chain.push({ span: span(statement), kind: "reexport", specifier: info.specifier, resolved_path: info.resolved_path });
        if (route(info.resolved_path, exportName)) { typeOnly ||= !!statement.isTypeOnly; moduleStatuses.push(info.status); externalPackage ||= info.external_package; return true; }
        chain.splice(before);
      }
      return false;
    }
    for (const step of [...chain]) if (step.kind === "import" && step.resolved_path) {
      const symbol = checker.getSymbolAtLocation(expression);
      const importName = symbol?.declarations?.find(ts.isImportSpecifier);
      route(step.resolved_path, importName ? (importName.propertyName || importName.name).text : ts.isPropertyAccessExpression(expression) ? expression.name.text : expression.getText());
    }
    const candidates = [...new Set(terminal.map(([id]) => id))].sort();
    const bodies = terminal.filter(([id]) => declarations.get(id).has_body && !declarations.get(id).declaration_only);
    let status = "unresolved";
    let reason = "The reference has no uniquely established function body";
    let target = null;
    const direct = ts.isIdentifier(expression) || ts.isPropertyAccessExpression(expression);
    const callable = bodies.filter(([, declaration]) => ts.isFunctionDeclaration(declaration) || (ts.isVariableDeclaration(declaration) && (declaration.parent.flags & ts.NodeFlags.Const) && (ts.isArrowFunction(declaration.initializer) || ts.isFunctionExpression(declaration.initializer))) || (ts.isExportAssignment(declaration) && !declaration.isExportEquals && (ts.isArrowFunction(declaration.expression) || ts.isFunctionExpression(declaration.expression))));
    const namespace = ts.isPropertyAccessExpression(expression) && checker.getSymbolAtLocation(expression.expression)?.declarations?.some(ts.isNamespaceImport);
    const assignedTarget = terminal.some(([, declaration]) => declaration.name && writtenSymbols.has(checker.getSymbolAtLocation(declaration.name)));
    if (typeOnly) { status = "unsupported_dispatch"; reason = "A type-only import or re-export does not establish a runtime callable value"; }
    else if (assignedTarget) { status = "unsupported_dispatch"; reason = "The referenced function binding is reassigned; its declaration does not establish the invocation target"; }
    else if (moduleStatuses.includes("runtime_mismatch")) { status = "runtime_mismatch"; reason = "Compiler and explicitly declared runtime alias targets disagree"; }
    else if (externalPackage) { status = "external_package"; reason = `Unknown behavior of a call from external package ${externalPackage}`; }
    else if (moduleStatuses.includes("missing_dependency")) { status = "missing_dependency"; reason = "An internal imported module is missing from the snapshot"; }
    else if (moduleStatuses.includes("excluded_body") || bodies.some(([id]) => declarations.get(id).excluded)) { status = "excluded_body"; reason = "The target source body is excluded from analysis"; }
    else if (terminal.length && terminal.every(([id]) => declarations.get(id).declaration_only)) { status = "declaration_only"; reason = "Only type declarations are available; runtime effects and return origins are unknown"; }
    else if (!direct || ts.isNewExpression(node) || (ts.isPropertyAccessExpression(expression) && !namespace)) { status = "unsupported_dispatch"; reason = "A type or method signature does not establish runtime dispatch"; }
    else if (callable.length === 1 && bodies.length === 1) { status = "resolved_body"; target = callable[0][0]; reason = "A direct reference resolves to one source body under the declared project configuration"; }
    else if (callable.length > 1) { status = "ambiguous"; reason = "Multiple source bodies remain possible"; }
    if (target) chain.push({ span: declarations.get(target).span, kind: "declaration", specifier: null, resolved_path: declarations.get(target).span.path });
    const call = { span: span(node), callee_span: span(expression), callee: expression.getText(), status, target, candidates, external_package: externalPackage, signature: signatureInfo, chain: chain.filter(step => step.span), reason };
    if (status !== "resolved_body") diagnostics.push({ code: status === "missing_dependency" || status === "excluded_body" ? "missing_dependency" : status === "external_package" ? "unknown_external_effect" : "unresolved_call", message: `${call.callee}: ${reason}`, snapshot: request.snapshot, frontier: null, span: call.span, affected_flows: ["project_metadata"] });
    return call;
  }
  for (const source of sourceFiles) visit(source);
  const sortSpan = (a, b) => compare(a.span.path, b.span.path) || a.span.byte_start - b.span.byte_start || a.span.byte_end - b.span.byte_end;
  const configurationFiles = Object.fromEntries([...configurations.keys()].sort().map(name => [name, request.files[name] || request.options.supplied_files[name]]));
  return {
    protocol_version: PROTOCOL, compiler_version: ts.version, snapshot: request.snapshot,
    tsconfig: configPath, configuration_files: configurationFiles, platform: request.options.platform,
    selected_paths: request.selected_paths, excluded_paths: request.options.excluded_paths, runtime_aliases: request.options.runtime_aliases,
    compiler_options: options,
    declarations: [...declarations.values()].sort(sortSpan), imports: imports.sort(sortSpan), calls: calls.sort(sortSpan), diagnostics: diagnostics.sort(sortSpan),
  };
}

function matchPattern(pattern, name) {
  if (!pattern.includes("*")) return pattern === name ? "" : undefined;
  const [prefix, suffix] = pattern.split("*");
  return name.length >= prefix.length + suffix.length && name.startsWith(prefix) && name.endsWith(suffix) ? name.slice(prefix.length, name.length - suffix.length) : undefined;
}
function findMapping(mappings, name) {
  const keys = Object.keys(mappings).sort((a, b) => (a.includes("*") - b.includes("*")) || b.indexOf("*") - a.indexOf("*") || compare(a, b));
  for (const key of keys) {
    const captured = matchPattern(key, name);
    if (captured !== undefined) return mappings[key].replace("*", captured);
  }
  return undefined;
}
function packageName(specifier) {
  return specifier.startsWith("@") ? specifier.split("/").slice(0, 2).join("/") : specifier.split("/")[0];
}

let request;
try {
  request = JSON.parse(fs.readFileSync(0, "utf8"));
  process.stdout.write(JSON.stringify({ metadata: analyze(request) }));
} catch (error) {
  const structured = error.code && error.snapshot ? error : {
    code: "provider_failure", snapshot: request?.snapshot || null,
    message: String(error.message || error), requested_by: null, specifier: null, candidates: [],
  };
  process.stdout.write(JSON.stringify({ error: structured }));
}
