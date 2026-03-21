const path = require("node:path");
const { MakerZIP } = require("@electron-forge/maker-zip");

module.exports = {
  packagerConfig: {
    executableName: "mxr-desktop",
    ignore: [/\/resources\/bin(?:\/|$)/],
    extraResource: [path.resolve(__dirname, "resources", "bin")],
  },
  makers: [new MakerZIP({}, ["darwin", "linux"])],
};
