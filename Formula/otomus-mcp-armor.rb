class OtomusMcpArmor < Formula
  desc "Capability enforcement for MCP tools — kernel-level sandboxing"
  homepage "https://github.com/otomus/mcparmor"
  version "0.3.1"
  license "MIT"

  if Hardware::CPU.arm?
    url "https://github.com/otomus/mcparmor/releases/download/v0.3.1/mcparmor-macos-arm64"
    sha256 "a701c520ff95f45cde02e24cbacbc31cc6c96b92a5d8980b3d96c209380694ab"
  else
    url "https://github.com/otomus/mcparmor/releases/download/v0.3.1/mcparmor-macos-x86_64"
    sha256 "eb27a5e2fc40d1293758df1dcc05007a045f739f65921b07f73d17c2d1dde34b"
  end

  def install
    binary_name = Hardware::CPU.arm? ? "mcparmor-macos-arm64" : "mcparmor-macos-x86_64"
    bin.install binary_name => "mcparmor"
  end

  test do
    assert_match "mcparmor", shell_output("#{bin}/mcparmor --version 2>&1", 0)
  end
end
