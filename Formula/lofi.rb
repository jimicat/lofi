class Lofi < Formula
  desc "Terminal radio for lo-fi, ambient and sleep music"
  homepage "https://github.com/jimicat/lofi"
  url "https://github.com/jimicat/lofi/archive/refs/tags/v0.1.0.tar.gz"
  version "0.1.0"
  sha256 "ea6b423f154a8a0907100d205aedbc525a816799e83d546324a23be728da3b04"
  head "https://github.com/jimicat/lofi.git", branch: "main"

  depends_on "rust" => :build

  on_linux do
    depends_on "pkgconf" => :build
    depends_on "alsa-lib"
  end

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "LOFI RADIO", shell_output("#{bin}/lofi --help")
  end
end
