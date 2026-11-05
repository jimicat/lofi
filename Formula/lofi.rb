class Lofi < Formula
  desc "Terminal radio for lo-fi, ambient and sleep music"
  homepage "https://github.com/jimicat/lofi"
  url "https://github.com/jimicat/lofi/releases/download/v0.1.0/lofi-0.1.0-x86_64-unknown-linux-gnu.tar.gz"
  version "0.1.0"
  sha256 "ffbd7acf31d68782f396296e0e1cff3c35da68c1a7c942ac7f60723aceaef2d3"

  on_macos do
    on_arm do
      url "https://github.com/jimicat/lofi/releases/download/v0.1.0/lofi-0.1.0-aarch64-apple-darwin.tar.gz"
      sha256 "8541e99e0b1971dee5500a5a434acda3e6c3e2504db1a782ff53cb893d330b8d"
    end

    on_intel do
      url "https://github.com/jimicat/lofi/releases/download/v0.1.0/lofi-0.1.0-x86_64-apple-darwin.tar.gz"
      sha256 "d0ba498fc6380e3b75483fd68b37d805888efefca1dfea26c767d9a7e8d2aef5"
    end
  end

  on_linux do
    depends_on "alsa-lib"
    depends_on arch: :x86_64
  end

  def install
    bin.install Dir["lofi-*/lofi"].first
  end

  test do
    assert_match "LOFI RADIO", shell_output("#{bin}/lofi --help")
  end
end
