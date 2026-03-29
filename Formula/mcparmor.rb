class Mcparmor < Formula
  desc "Capability enforcement for MCP tools — kernel-level sandboxing"
  homepage "https://github.com/otomus/mcparmor"
  version "0.1.0"
  license "MIT"

  if Hardware::CPU.arm?
    url "https://github.com/otomus/mcparmor/releases/download/v0.1.0/mcparmor-macos-arm64"
    sha256 "PLACEHOLDER_ARM64_SHA256"
  else
    url "https://github.com/otomus/mcparmor/releases/download/v0.1.0/mcparmor-macos-x86_64"
    sha256 "PLACEHOLDER_X86_64_SHA256"
  end

  def install
    binary_name = Hardware::CPU.arm? ? "mcparmor-macos-arm64" : "mcparmor-macos-x86_64"
    bin.install binary_name => "mcparmor"
  end

  test do
    assert_match "mcparmor", shell_output("#{bin}/mcparmor --version 2>&1", 0)
  end
end
