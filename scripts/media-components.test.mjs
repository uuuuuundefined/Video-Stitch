import assert from "node:assert/strict";
import { test } from "node:test";
import { mediaTarget, validateMediaBinary } from "./media-components.mjs";

// 仅用于验证 PE 解析边界的内存夹具，不落入组件暂存区或安装包。
function peFixture(dll = "KERNEL32.dll") {
  const bytes = Buffer.alloc(1024);
  bytes.write("MZ");
  bytes.writeUInt32LE(64, 0x3c);
  bytes.write("PE\0\0", 64);
  bytes.writeUInt16LE(0x8664, 68);
  bytes.writeUInt16LE(1, 70);
  bytes.writeUInt16LE(240, 84);
  bytes.writeUInt16LE(0x20b, 88);
  bytes.writeUInt32LE(512, 148);
  bytes.writeUInt32LE(16, 196);
  bytes.writeUInt32LE(0x1000, 208);
  bytes.writeUInt32LE(0x1000, 340);
  bytes.writeUInt32LE(512, 344);
  bytes.writeUInt32LE(512, 348);
  bytes.writeUInt32LE(0x1040, 524);
  bytes.write(dll + "\0", 576);
  return bytes;
}

test("host and explicit target must be supported", () => {
  assert.equal(mediaTarget(null, "win32", "x64"), "x86_64-pc-windows-msvc");
  assert.throws(() => mediaTarget("aarch64-pc-windows-msvc"));
});

test("Windows x64 static component accepts system DLLs", () => {
  validateMediaBinary(peFixture(), "x86_64-pc-windows-msvc");
});

test("Windows staging rejects external runtimes and wrong architecture", () => {
  assert.throws(
    () =>
      validateMediaBinary(
        peFixture("libwinpthread-1.dll"),
        "x86_64-pc-windows-msvc",
      ),
    /non-system DLL/,
  );
  const bytes = peFixture();
  bytes.writeUInt16LE(0xaa64, 68);
  assert.throws(
    () => validateMediaBinary(bytes, "x86_64-pc-windows-msvc"),
    /x64/,
  );
});

test("malformed import addresses and renamed non-PE files are rejected", () => {
  assert.throws(() =>
    validateMediaBinary(Buffer.from("not an exe"), "x86_64-pc-windows-msvc"),
  );
  const bytes = peFixture();
  bytes.writeUInt32LE(0xffffff00, 524);
  assert.throws(
    () => validateMediaBinary(bytes, "x86_64-pc-windows-msvc"),
    /import address/,
  );
});
