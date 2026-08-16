import { describe, expect, it } from "vitest";
import { collectLinkTargets, stripFencedBlocks } from "../../scripts/doc-links.mjs";

describe("collecting link targets", () => {
  it("collects a relative inline link", () => {
    expect(collectLinkTargets("See [the guide](docs/AGENT_GUIDE.md).")).toEqual([
      "docs/AGENT_GUIDE.md",
    ]);
  });

  it("collects the path part of a link with an anchor", () => {
    expect(collectLinkTargets("[status](adr/README.md#status-values)")).toEqual(["adr/README.md"]);
  });

  it("collects a reference definition", () => {
    expect(collectLinkTargets("[guide]: ../CONTRIBUTING.md\n")).toEqual(["../CONTRIBUTING.md"]);
  });

  it("collects an image target", () => {
    expect(collectLinkTargets("![shot](docs/evidence/shot.png)")).toEqual([
      "docs/evidence/shot.png",
    ]);
  });

  it("decodes an escaped space", () => {
    expect(collectLinkTargets("[x](docs/a%20b.md)")).toEqual(["docs/a b.md"]);
  });

  it("skips external and same-document links", () => {
    const document = [
      "[web](https://example.invalid/x)",
      "[mail](mailto:someone@example.invalid)",
      "[anchor](#status-values)",
      "[scheme-relative](//example.invalid/x)",
    ].join("\n");
    expect(collectLinkTargets(document)).toEqual([]);
  });

  it("ignores link-shaped text inside a fenced block", () => {
    const document = ["```markdown", "[template](docs/does-not-exist.md)", "```", ""].join("\n");
    expect(collectLinkTargets(document)).toEqual([]);
  });

  it("resumes collecting after a fenced block closes", () => {
    const document = ["```", "not a link", "```", "[real](docs/ARCHITECTURE.md)"].join("\n");
    expect(collectLinkTargets(document)).toEqual(["docs/ARCHITECTURE.md"]);
  });
});

describe("stripping fenced blocks", () => {
  it("keeps line numbering stable", () => {
    const document = ["one", "```", "two", "```", "five"].join("\n");
    expect(stripFencedBlocks(document).split("\n")).toEqual(["one", "", "", "", "five"]);
  });

  it("handles a tilde fence", () => {
    const document = ["~~~", "[x](nowhere.md)", "~~~"].join("\n");
    expect(collectLinkTargets(document)).toEqual([]);
  });
});
