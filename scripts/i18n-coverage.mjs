import ts from "typescript";

// Product names, the author credit, language endonyms and standard units stay literal.
const invariantText = new Set([
  "PP-OCRv6",
  "Windows OCR",
  "GitHub ↗",
  "Shanxin Li (@PeterShanxin)",
  "English",
  "简体中文",
  "- Dev",
  "px",
  "ms",
  "GB",
]);
const expressionMarker = "\uE000";
const normalizeText = (text) => text.replaceAll(expressionMarker, "").replace(/\s+/g, " ").trim();
const hasWords = (text) => /[\p{L}]/u.test(text);
const literalText = (node) =>
  ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node) ? node.text : null;

function unwrap(node) {
  while (
    ts.isAsExpression(node) ||
    ts.isSatisfiesExpression(node) ||
    ts.isParenthesizedExpression(node)
  )
    node = node.expression;
  return node;
}

/** Read the namespaced TypeScript catalog as data; never execute catalog source. */
export function readOverlayMessages(source) {
  const file = ts.createSourceFile("overlay-messages.ts", source, ts.ScriptTarget.Latest, true);
  let initializer;
  for (const statement of file.statements) {
    if (!ts.isVariableStatement(statement)) continue;
    for (const declaration of statement.declarationList.declarations) {
      if (declaration.name.getText(file) === "overlayMessages")
        initializer = declaration.initializer;
    }
  }
  function object(node) {
    if (!node || !ts.isObjectLiteralExpression(unwrap(node)))
      throw new Error("overlayMessages must contain only literal message objects");
    return Object.fromEntries(
      unwrap(node).properties.map((property) => {
        if (!ts.isPropertyAssignment(property) || ts.isComputedPropertyName(property.name))
          throw new Error("overlayMessages cannot contain computed keys, methods or spreads");
        const key = ts.isIdentifier(property.name)
          ? property.name.text
          : literalText(property.name);
        if (key === null) throw new Error("overlayMessages keys must be strings");
        const value = literalText(property.initializer);
        return [key, value === null ? object(property.initializer) : value];
      }),
    );
  }
  return object(initializer);
}

export function makeCatalog(chinese, overlay) {
  const messages = Object.fromEntries(
    Object.entries(chinese).map(([en, zh]) => [en, { en, "zh-CN": zh }]),
  );
  for (const [key, message] of Object.entries(overlay)) {
    if (Object.hasOwn(messages, key)) throw new Error(`Duplicate message key: ${key}`);
    messages[key] = message;
  }
  return messages;
}

export function validateCatalog(messages) {
  const problems = [];
  const placeholders = (text) =>
    [...text.matchAll(/\{(\w+)\}/g)]
      .map((match) => match[1])
      .sort()
      .join(",");
  for (const [key, entry] of Object.entries(messages)) {
    for (const locale of ["en", "zh-CN"]) {
      if (typeof entry?.[locale] !== "string" || !entry[locale].trim())
        problems.push(`catalog: ${key}: missing or empty ${locale} message`);
    }
    if (
      typeof entry?.en === "string" &&
      typeof entry["zh-CN"] === "string" &&
      placeholders(entry.en) !== placeholders(entry["zh-CN"])
    )
      problems.push(`catalog: ${key}: named placeholders differ between languages`);
  }
  return problems;
}

/** Inspect literal calls and static render slots; runtime/model strings remain data. */
export function checkSource(path, source, messages) {
  const problems = [];
  const reportMissing = (key, line) => {
    if (!Object.hasOwn(messages, key))
      problems.push(`${path}:${line}: missing translation key ${JSON.stringify(key)}`);
  };
  if (path.endsWith(".html")) {
    const html = source.replace(/<!--[\s\S]*?-->/g, (comment) => comment.replace(/[^\n]/g, " "));
    for (const match of html.matchAll(
      /\bdata-i18n(?:-(?:aria-label|title|placeholder|alt))?\s*=\s*(["'])(.*?)\1/g,
    ))
      reportMissing(match[2], html.slice(0, match.index).split("\n").length);
    return problems;
  }
  const file = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true);
  const lineAt = (node) => file.getLineAndCharacterOfPosition(node.getStart(file)).line + 1;
  const reportRaw = (value, node) => {
    const text = normalizeText(value);
    if (hasWords(text) && !invariantText.has(text))
      problems.push(`${path}:${lineAt(node)}: untranslated visible text ${JSON.stringify(text)}`);
  };
  function renderedExpression(node) {
    const value = literalText(node);
    if (value !== null) reportRaw(value, node);
    else if (ts.isConditionalExpression(node)) {
      renderedExpression(node.whenTrue);
      renderedExpression(node.whenFalse);
    } else if (
      ts.isBinaryExpression(node) &&
      [
        ts.SyntaxKind.QuestionQuestionToken,
        ts.SyntaxKind.BarBarToken,
        ts.SyntaxKind.AmpersandAmpersandToken,
      ].includes(node.operatorToken.kind)
    ) {
      renderedExpression(node.right);
    } else if (ts.isParenthesizedExpression(node)) renderedExpression(node.expression);
  }
  function template(node) {
    const value = node.template;
    let markup = ts.isNoSubstitutionTemplateLiteral(value) ? value.text : value.head.text;
    if (ts.isTemplateExpression(value)) {
      for (const span of value.templateSpans) {
        const inTag = markup.lastIndexOf("<") > markup.lastIndexOf(">");
        if (!inTag || /\b(?:aria-label|title|placeholder|alt)\s*=\s*["']?$/.test(markup))
          renderedExpression(span.expression);
        markup += expressionMarker + span.literal.text;
      }
    }
    markup = markup.replace(/<!--[\s\S]*?-->/g, (comment) => comment.replace(/[^\n]/g, " "));
    for (const match of markup.matchAll(/<[^>]*>|[^<]+/g)) {
      if (!match[0].startsWith("<")) reportRaw(match[0], node);
      else
        for (const attribute of match[0].matchAll(
          /\b(?:aria-label|title|placeholder|alt)\s*=\s*(["'])(.*?)\1/g,
        ))
          reportRaw(attribute[2], node);
    }
  }
  function visit(node) {
    if (
      ts.isCallExpression(node) &&
      ts.isIdentifier(node.expression) &&
      node.expression.text === "t" &&
      node.arguments.length
    ) {
      const key = literalText(node.arguments[0]);
      if (key !== null) reportMissing(key, lineAt(node));
    }
    if (
      ts.isTaggedTemplateExpression(node) &&
      ts.isIdentifier(node.tag) &&
      node.tag.text === "html"
    )
      template(node);
    ts.forEachChild(node, visit);
  }
  visit(file);
  return problems;
}
