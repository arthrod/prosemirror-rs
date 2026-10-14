export function downloadText(filename: string, content: string, type: string) {
  const blob = new Blob([content], { type: `${type};charset=utf-8` });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

export function standaloneHtml(title: string, bodyHtml: string) {
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escapeHtml(title)}</title>
<style>
  body { margin: 0; background: #f3efe6; color: #1c1917; font-family: Georgia, "Iowan Old Style", serif; }
  main { max-width: 42rem; margin: 0 auto; padding: 3rem 1.5rem 4rem; }
  h1,h2,h3 { font-weight: 560; line-height: 1.2; }
  p, li { line-height: 1.55; }
  pre { background: #ebe4d8; padding: 0.8rem 1rem; overflow: auto; }
  code { font-family: ui-monospace, monospace; font-size: 0.92em; }
  table { border-collapse: collapse; width: 100%; }
  td, th { border: 1px solid #d9d0c2; padding: 0.35rem 0.5rem; text-align: left; }
  blockquote { margin-left: 0; padding-left: 1rem; border-left: 3px solid #9a3412; }
  img { max-width: 100%; }
  .pm-image-blocked { display: inline-block; padding: 0.2rem 0.4rem; background: #f6e6dc; }
</style>
</head>
<body>
<main>
${bodyHtml}
</main>
</body>
</html>
`;
}

function escapeHtml(value: string) {
  return value.replace(/[&<>"']/g, (char) => {
    switch (char) {
      case "&":
        return "&" + "amp;";
      case "<":
        return "&" + "lt;";
      case ">":
        return "&" + "gt;";
      case '"':
        return "&" + "quot;";
      default:
        return "&" + "#39;";
    }
  });
}
