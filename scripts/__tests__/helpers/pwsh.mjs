export const psLiteral = value => `'${value.replaceAll("'", "''")}'`;

export function pwshArgs(script, cwd) {
  // The Windows runner loses non-ASCII paths in raw argv and startup cwd.
  // Keep argv ASCII; restore the fixture directory inside PowerShell.
  const command = `$ErrorActionPreference = 'Stop';\n` +
    `[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false);\n` +
    `Set-Location -LiteralPath ${psLiteral(cwd)};\n${script}`;
  return ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass",
    "-OutputFormat", "Text", "-EncodedCommand",
    Buffer.from(command, "utf16le").toString("base64")];
}
