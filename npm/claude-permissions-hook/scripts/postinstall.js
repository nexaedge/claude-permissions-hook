const os = require("os");
const path = require("path");

const PLATFORMS = {
  "darwin-arm64": "claude-permissions-hook-darwin-arm64",
  "darwin-x64": "claude-permissions-hook-darwin-x64",
  "linux-x64": "claude-permissions-hook-linux-x64",
  "linux-arm64": "claude-permissions-hook-linux-arm64",
};

const platformKey = `${os.platform()}-${os.arch()}`;
const pkg = PLATFORMS[platformKey];

if (!pkg) {
  console.warn(
    `claude-permissions-hook: unsupported platform ${platformKey}. ` +
      `Supported: ${Object.keys(PLATFORMS).join(", ")}`
  );
  process.exit(0);
}

try {
  const pkgPath = path.dirname(require.resolve(`${pkg}/package.json`));
  const binPath = path.join(pkgPath, "bin", "claude-permissions-hook");
  require("fs").accessSync(binPath, require("fs").constants.X_OK);
} catch {
  console.warn(
    `claude-permissions-hook: platform package ${pkg} not installed. ` +
      `The binary may not work. Try: npm install claude-permissions-hook`
  );
}
