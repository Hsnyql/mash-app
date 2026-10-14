export const psLiteral = value => `'${value.replaceAll("'", "''")}'`;

export function pwshArgs(script, cwd) {
  // Keep Unicode fixture paths out of raw argv and set the intended cwd
  // explicitly, independent of PowerShell's startup directory handling.
  const command = `$ErrorActionPreference = 'Stop';\n` +
    `[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false);\n` +
    `Set-Location -LiteralPath ${psLiteral(cwd)};\n${script}`;
  return ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass",
    "-OutputFormat", "Text", "-EncodedCommand",
    Buffer.from(command, "utf16le").toString("base64")];
}
