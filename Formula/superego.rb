class Superego < Formula
  desc "Superego - Metacognitive advisor for Claude Code"
  homepage "https://github.com/cloud-atlas-ai/superego"
  url "https://github.com/cloud-atlas-ai/superego/archive/refs/tags/v0.3.1.tar.gz"
  sha256 "094979f491d331d177143f875f957336116115788fc3496acbca90a34b57bffb"
  license :cannot_represent

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "sg", shell_output("#{bin}/sg --help")
  end
end
