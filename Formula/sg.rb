class Sg < Formula
  desc "Superego - Metacognitive advisor for Claude Code"
  homepage "https://github.com/OWNER/higher-peak"
  url "https://github.com/OWNER/higher-peak/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "PLACEHOLDER"  # TODO: Update with actual sha256 after release
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "sg", shell_output("#{bin}/sg --help")
  end
end
