class PactLang < Formula
  desc "A typed, permission-enforced language for orchestrating AI agents"
  homepage "https://pactlang.dev"
  url "https://github.com/pact-lang/pact/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "PLACEHOLDER_SHA256"
  license "MIT"
  head "https://github.com/pact-lang/pact.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: "crates/pact-cli")
    # Install the LSP binary
    system "cargo", "install", *std_cargo_args(path: "crates/pact-lsp")
  end

  test do
    # Verify the CLI runs
    assert_match "pact-lang", shell_output("#{bin}/pact --version")

    # Write a minimal .pact file and check it
    (testpath/"hello.pact").write <<~PACT
      permit_tree {
          ^llm { ^llm.query }
      }

      tool #greet {
          description: <<Generate a greeting.>>
          requires: [^llm.query]
          params { name :: String }
          returns :: String
      }

      agent @greeter {
          permits: [^llm.query]
          tools: [#greet]
          prompt: <<You are friendly.>>
      }

      flow hello(name :: String) -> String {
          result = @greeter -> #greet(name)
          return result
      }
    PACT

    assert_match "no errors", shell_output("#{bin}/pact check #{testpath}/hello.pact")
  end
end
