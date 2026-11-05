class Lofi < Formula
  desc "Terminal radio for lo-fi, ambient and sleep music"
  homepage "https://github.com/jimicat/lofi"
  url "https://github.com/jimicat/lofi/archive/50b8560a74cb5d8cde4624746e170bd0028b9421.tar.gz"
  version "0.1.0"
  sha256 "a3a387b6835bac20bc040029eb0804f6c29cc2c510012a37e0143c2fd9d2a3bc"
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
