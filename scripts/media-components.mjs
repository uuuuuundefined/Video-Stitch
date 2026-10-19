const targets = new Set(["aarch64-apple-darwin", "x86_64-pc-windows-msvc"]);

// 解析发布目标，不允许未知架构或以宿主默认值掩盖显式目标。
export function mediaTarget(
  explicit,
  platform = process.platform,
  arch = process.arch,
) {
  const target =
    explicit ||
    (platform === "darwin" && arch === "arm64"
      ? "aarch64-apple-darwin"
      : platform === "win32" && arch === "x64"
        ? "x86_64-pc-windows-msvc"
        : null);
  if (!targets.has(target))
    throw new Error(`Unsupported media target: ${target}`);
  return target;
}

const systemDlls = new Set([
  "kernel32.dll",
  "ntdll.dll",
  "msvcrt.dll",
  "ucrtbase.dll",
  "advapi32.dll",
  "bcrypt.dll",
  "crypt32.dll",
  "gdi32.dll",
  "user32.dll",
  "shell32.dll",
  "ole32.dll",
  "oleaut32.dll",
  "ws2_32.dll",
  "secur32.dll",
  "shlwapi.dll",
  "winmm.dll",
  "version.dll",
  "avicap32.dll",
  "vfw32.dll",
  "psapi.dll",
  "setupapi.dll",
  "imm32.dll",
  "comdlg32.dll",
  "comctl32.dll",
  "dwmapi.dll",
]);

// 检查 PE 架构及导入表，拒绝外部编译器运行库和非系统 DLL。
export function validateMediaBinary(bytes, target) {
  if (target === "aarch64-apple-darwin") {
    if (
      bytes.length < 8 ||
      bytes.readUInt32LE(0) !== 0xfeedfacf ||
      bytes.readUInt32LE(4) !== 0x0100000c
    )
      throw new Error("Expected an arm64 Mach-O media executable");
    return;
  }
  if (target !== "x86_64-pc-windows-msvc")
    throw new Error("Unsupported media target");
  if (bytes.length < 64 || bytes.toString("ascii", 0, 2) !== "MZ")
    throw new Error("Expected a Windows PE media executable");
  const pe = bytes.readUInt32LE(0x3c);
  if (
    pe + 24 > bytes.length ||
    bytes.toString("ascii", pe, pe + 4) !== "PE\0\0" ||
    bytes.readUInt16LE(pe + 4) !== 0x8664
  )
    throw new Error("Expected a Windows x64 PE media executable");
  const optional = pe + 24;
  const optionalSize = bytes.readUInt16LE(pe + 20);
  if (
    optionalSize < 240 ||
    optional + optionalSize > bytes.length ||
    bytes.readUInt16LE(optional) !== 0x20b ||
    bytes.readUInt32LE(optional + 108) < 14
  )
    throw new Error("Invalid PE32+ optional header");
  if (bytes.readUInt32LE(optional + 216) !== 0)
    throw new Error("Delayed DLL imports require a separate dependency audit");
  const sections = optional + optionalSize;
  const count = bytes.readUInt16LE(pe + 6);
  if (sections + count * 40 > bytes.length)
    throw new Error("Invalid PE sections");
  // RVA 必须映射到实际文件内容，拒绝越界或仅存在于内存中的数据。
  const offset = (rva) => {
    if (rva < bytes.readUInt32LE(optional + 60) && rva < bytes.length)
      return rva;
    for (let i = 0; i < count; i++) {
      const section = sections + i * 40;
      const address = bytes.readUInt32LE(section + 12);
      const size = bytes.readUInt32LE(section + 16);
      if (rva >= address && rva - address < size) {
        const position = bytes.readUInt32LE(section + 20) + rva - address;
        if (position < bytes.length) return position;
      }
    }
    throw new Error("Invalid PE import address");
  };
  const imports = bytes.readUInt32LE(optional + 120);
  if (!imports) return;
  for (let i = 0; i < 512; i++) {
    const descriptor = offset(imports + i * 20);
    if (descriptor + 20 > bytes.length)
      throw new Error("Invalid PE import descriptor");
    if (bytes.subarray(descriptor, descriptor + 20).every((byte) => byte === 0))
      return;
    const start = offset(bytes.readUInt32LE(descriptor + 12));
    const end = bytes.indexOf(0, start);
    if (end < start || end - start > 260)
      throw new Error("Invalid PE import name");
    const dll = bytes.toString("ascii", start, end).toLowerCase();
    if (
      !systemDlls.has(dll) &&
      !/^(api-ms-win-|ext-ms-win-)[a-z0-9-]+\.dll$/.test(dll)
    )
      throw new Error(`Media executable depends on non-system DLL: ${dll}`);
  }
  throw new Error("Unterminated PE import table");
}
