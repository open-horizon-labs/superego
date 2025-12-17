class Superego < Formula
  desc "Superego - Metacognitive advisor for Claude Code"
  homepage "https://github.com/cloud-atlas-ai/superego"
  url "https://github.com/cloud-atlas-ai/superego/archive/refs/tags/v0.3.3.tar.gz"
  sha256 "74644f76b23c5ff4f9fbd39bd99b3359c632f3bc1ad6a7a3d456d4b54e1961ae"
  license :cannot_represent

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "sg", shell_output("#{bin}/sg --help")
  end
end
