# The Homebrew cask for claudebar.
#
# The template for the tap. On each `v*` tag `.github/workflows/release.yml`
# builds the universal .app, fills `version` and `sha256` in below, and pushes
# the result to github.com/gautham-v/homebrew-tap as `Casks/claudebar.rb`;
# users install with `brew install --cask gautham-v/tap/claudebar`.
# The values here are the last release's and need no hand edits.
cask "claudebar" do
  version "0.1.0"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"

  url "https://github.com/gautham-v/claudebar/releases/download/v#{version}/Claudebar-#{version}.zip"
  name "Claudebar"
  desc "Claude usage limits in the menu bar"
  homepage "https://github.com/gautham-v/claudebar"

  # SMAppService, which launch-at-login uses, is macOS 13+.
  depends_on macos: ">= :ventura"

  app "Claudebar.app"

  # The build is not notarized, so Gatekeeper refuses the first launch.
  caveats <<~EOS
    The app is signed ad hoc, not notarized. On first launch macOS will say it
    cannot verify the developer. Open it anyway with:

      System Settings → Privacy & Security → Open Anyway

    or clear the quarantine flag and launch it:

      xattr -dr com.apple.quarantine /Applications/Claudebar.app
      open /Applications/Claudebar.app

    Then turn on "Launch at login" from the popover.
  EOS

  uninstall quit: "com.gauthamv.claudebar"

  zap trash: [
    "~/.config/claudebar",
    "~/Library/Caches/claudebar",
  ]
end
