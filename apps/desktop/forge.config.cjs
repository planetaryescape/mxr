const path = require("node:path");
const fs = require("node:fs");
const { execFileSync } = require("node:child_process");

const appName = "mxr";
const executableName = "mxr-desktop";
const homepage = "https://github.com/planetaryescape/mxr";
const iconBase = findIconBase();
const pngIcon = iconBase && fs.existsSync(`${iconBase}.png`) ? `${iconBase}.png` : undefined;
const icnsIcon = iconBase && fs.existsSync(`${iconBase}.icns`) ? `${iconBase}.icns` : undefined;
const osxSign = macSignConfig();
const osxNotarize = osxSign ? macNotarizeConfig() : undefined;

function compact(value) {
  return Object.fromEntries(Object.entries(value).filter(([, entry]) => entry !== undefined));
}

function findIconBase() {
  const candidates = [
    path.resolve(__dirname, "resources", "icon"),
    path.resolve(__dirname, "resources", "icons", "icon"),
  ];

  return candidates.find((candidate) =>
    [".icns", ".png"].some((extension) => fs.existsSync(`${candidate}${extension}`)),
  );
}

function macSignConfig() {
  const identity = process.env.MACOS_SIGN_IDENTITY || process.env.APPLE_SIGNING_IDENTITY;
  const enabled = process.env.MXR_MACOS_SIGN === "true" || Boolean(identity);
  if (!enabled) {
    return undefined;
  }

  return compact({
    identity,
    hardenedRuntime: true,
    gatekeeperAssess: false,
  });
}

function macNotarizeConfig() {
  if (process.env.APPLE_API_KEY && process.env.APPLE_API_KEY_ID && process.env.APPLE_API_ISSUER) {
    return {
      appleApiKey: process.env.APPLE_API_KEY,
      appleApiKeyId: process.env.APPLE_API_KEY_ID,
      appleApiIssuer: process.env.APPLE_API_ISSUER,
    };
  }

  const appleIdPassword = process.env.APPLE_PASSWORD || process.env.APPLE_ID_PASSWORD;
  if (process.env.APPLE_ID && appleIdPassword && process.env.APPLE_TEAM_ID) {
    return {
      appleId: process.env.APPLE_ID,
      appleIdPassword,
      teamId: process.env.APPLE_TEAM_ID,
    };
  }

  const keychainProfile = process.env.APPLE_KEYCHAIN_PROFILE || process.env.NOTARYTOOL_KEYCHAIN_PROFILE;
  if (keychainProfile) {
    return compact({
      keychainProfile,
      keychain: process.env.APPLE_KEYCHAIN,
    });
  }

  return undefined;
}

function ensureNativeHelper(packageName, artifactPath) {
  const packageRoot = path.resolve(__dirname, "node_modules", packageName);
  const artifact = path.resolve(packageRoot, artifactPath);
  const binding = path.resolve(packageRoot, "binding.gyp");
  if (fs.existsSync(artifact) || !fs.existsSync(binding)) {
    return;
  }

  const nodeGyp = path.resolve(
    __dirname,
    "node_modules",
    ".bin",
    process.platform === "win32" ? "node-gyp.cmd" : "node-gyp",
  );
  try {
    execFileSync(nodeGyp, ["rebuild"], {
      cwd: packageRoot,
      stdio: "pipe",
      env: compact({
        PATH: process.env.PATH,
        HOME: process.env.HOME,
        TMPDIR: process.env.TMPDIR,
        PYTHON: process.env.PYTHON,
        SDKROOT: process.env.SDKROOT,
        MACOSX_DEPLOYMENT_TARGET: process.env.MACOSX_DEPLOYMENT_TARGET,
        npm_config_cache: process.env.npm_config_cache,
        npm_config_node_gyp: process.env.npm_config_node_gyp,
        npm_config_python: process.env.npm_config_python,
        npm_config_ignore_scripts: "false",
        npm_config_loglevel: "error",
      }),
    });
  } catch (error) {
    throw new Error(`Failed to rebuild ${packageName} for DMG packaging`, { cause: error });
  }
}

function ensureDmgNativeHelpers() {
  if (process.platform !== "darwin") {
    return;
  }

  ensureNativeHelper("macos-alias", path.join("build", "Release", "volume.node"));
  ensureNativeHelper("fs-xattr", path.join("build", "Release", "xattr.node"));
}

const packagerConfig = compact({
  name: appName,
  executableName,
  appBundleId: "app.planetaryescape.mxr",
  appCategoryType: "public.app-category.productivity",
  icon: iconBase,
  ignore: [/\/resources\/bin(?:\/|$)/],
  extraResource: [path.resolve(__dirname, "resources", "bin")],
  osxSign,
  osxNotarize,
});

module.exports = {
  packagerConfig,
  makers: [
    {
      name: "@electron-forge/maker-zip",
      platforms: ["darwin", "linux"],
    },
    {
      name: "@electron-forge/maker-dmg",
      platforms: ["darwin"],
      config: compact({
        name: appName,
        icon: icnsIcon,
        format: "ULFO",
      }),
    },
    {
      name: "@electron-forge/maker-deb",
      platforms: ["linux"],
      config: {
        options: compact({
          name: executableName,
          productName: appName,
          genericName: "Email Client",
          maintainer: "Planetary Escape",
          homepage,
          categories: ["Utility", "Email"],
          icon: pngIcon,
        }),
      },
    },
    {
      name: "@electron-forge/maker-rpm",
      platforms: ["linux"],
      config: {
        options: compact({
          name: executableName,
          productName: appName,
          genericName: "Email Client",
          homepage,
          categories: ["Utility", "Email"],
          icon: pngIcon,
        }),
      },
    },
  ],
  publishers: [
    {
      name: "@electron-forge/publisher-github",
      config: {
        repository: {
          owner: "planetaryescape",
          name: "mxr",
        },
        draft: false,
        prerelease: process.env.MXR_DESKTOP_PRERELEASE === "true",
      },
    },
  ],
  hooks: {
    preMake: async () => {
      ensureDmgNativeHelpers();
    },
  },
};
