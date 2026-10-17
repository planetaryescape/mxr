cask "mxr-desktop" do
  version "__VERSION__"
  sha256 "__SHA256_MACOS_AARCH64__"

  url "https://github.com/planetaryescape/mxr/releases/download/v#{version}/mxr-desktop-#{version}-aarch64.dmg"
  name "mxr"
  desc "Desktop window for the local-first mxr email client"
  homepage "https://github.com/planetaryescape/mxr"

  depends_on arch: :arm64
  depends_on formula: "planetaryescape/mxr/mxr"
  depends_on macos: :monterey

  app "mxr.app"

  zap trash: [
    "~/Library/Application Support/dev.planetaryescape.mxr",
    "~/Library/Caches/dev.planetaryescape.mxr",
    "~/Library/Saved Application State/dev.planetaryescape.mxr.savedState",
    "~/Library/WebKit/dev.planetaryescape.mxr",
  ]
end
