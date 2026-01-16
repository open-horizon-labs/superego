class Superego < Formula
  desc "Superego - Metacognitive advisor for Claude Code"
  homepage "https://github.com/cloud-atlas-ai/superego"
  url "https://github.com/cloud-atlas-ai/superego/archive/refs/tags/v0.9.1.tar.gz"
  sha256 "3805f02659218a3736900497e09b5545fece59dd5423d99bf042b875f2e6e43e"
  license :cannot_represent

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "sg", shell_output("#{bin}/sg --help")
  end
end
