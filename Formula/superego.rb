class Superego < Formula
  desc "Superego - Metacognitive advisor for Claude Code"
  homepage "https://github.com/cloud-atlas-ai/superego"
  url "https://github.com/cloud-atlas-ai/superego/archive/refs/tags/v0.4.0.tar.gz"
  sha256 "b7212cba3ed9cb2b231233c9afb6d9558693f843ee786d62b1327f1c9f44b0b0"
  license :cannot_represent

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "sg", shell_output("#{bin}/sg --help")
  end
end
