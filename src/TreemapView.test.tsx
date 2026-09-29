import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { TreemapView } from "./TreemapView";
import type { TreemapNodeDatum } from "./types";

/// 单 child 树保证命中确定性：根只有一个目录矩形时，画布中心必中它。
const tree: TreemapNodeDatum = {
  id: "C:",
  name: "C:",
  sizeGb: 10,
  kind: "root",
  children: [
    {
      id: "C:\\Alpha",
      name: "Alpha",
      sizeGb: 10,
      kind: "dir",
      children: [
        { id: "C:\\Alpha\\beta.bin", name: "beta.bin", sizeGb: 8, kind: "file" },
        { id: "C:\\Alpha::residual", name: "其他（未列出）", sizeGb: 2, kind: "residual" }
      ]
    }
  ]
};

const canvas = () => screen.getByRole("img", { name: /只读体量图/ });

describe("TreemapView", () => {
  it("shows a tooltip on hover and clears it on leave", () => {
    render(<TreemapView data={tree} />);

    fireEvent.mouseMove(canvas(), { clientX: 360, clientY: 160 });
    const tooltip = screen.getByRole("status");
    expect(tooltip).toHaveTextContent("Alpha");
    expect(tooltip).toHaveTextContent("点击可下钻");

    fireEvent.mouseLeave(canvas());
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("drills into a directory and navigates back via breadcrumbs", () => {
    render(<TreemapView data={tree} />);

    fireEvent.click(canvas(), { clientX: 360, clientY: 160 });
    // 面包屑出现 Alpha，焦点尺寸文本跟随。
    expect(screen.getByRole("button", { name: "Alpha" })).toBeInTheDocument();
    expect(screen.getByText(/Alpha：10\.00 GB/)).toBeInTheDocument();
    // 下钻后 aria-label 更新为当前焦点。
    expect(canvas()).toHaveAccessibleName(/Alpha/);

    // 面包屑回到根。
    fireEvent.click(screen.getByRole("button", { name: "C:" }));
    expect(screen.getByText(/C:：10\.00 GB/)).toBeInTheDocument();
  });

  it("keeps tooltip read-only aggregated text without live disk access", () => {
    render(<TreemapView data={tree} />);
    fireEvent.click(canvas(), { clientX: 360, clientY: 160 });
    // 下钻后画布是 beta.bin + 残差两个矩形，中心必中其一。
    fireEvent.mouseMove(canvas(), { clientX: 360, clientY: 160 });
    const tooltip = screen.getByRole("status");
    expect(tooltip.textContent).toMatch(/beta\.bin|其他（未列出）/);
    expect(screen.getByText(/不做实时磁盘访问/)).toBeInTheDocument();
  });

  it("falls back to the root when focus points at a missing node after data change", () => {
    const { rerender } = render(<TreemapView data={tree} />);
    fireEvent.click(canvas(), { clientX: 360, clientY: 160 });
    expect(screen.getByRole("button", { name: "Alpha" })).toBeInTheDocument();

    // 换一份没有 Alpha 的报告：焦点自动回落到新根。
    const nextTree: TreemapNodeDatum = {
      id: "D:",
      name: "D:",
      sizeGb: 4,
      kind: "root",
      children: [{ id: "D:\\Only", name: "Only", sizeGb: 4, kind: "file" }]
    };
    rerender(<TreemapView data={nextTree} />);
    expect(screen.queryByRole("button", { name: "Alpha" })).not.toBeInTheDocument();
    expect(screen.getByText(/D:：4\.00 GB/)).toBeInTheDocument();
  });
});
