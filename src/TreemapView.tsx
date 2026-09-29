import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { hierarchy, treemap, treemapSquarify } from "d3-hierarchy";
import type { HierarchyRectangularNode } from "d3-hierarchy";
import { formatSize, treemapNodeDetail } from "./reportUtils";
import type { TreemapNodeDatum } from "./types";

interface TreemapViewProps {
  data: TreemapNodeDatum;
}

type LayoutNode = {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
  depth: number;
  datum: TreemapNodeDatum;
};

const KIND_COLORS: Record<TreemapNodeDatum["kind"], [string, string]> = {
  root: ["#2f4858", "#243842"],
  dir: ["#33658a", "#2a5272"],
  file: ["#55a630", "#468a28"],
  residual: ["#8d99ae", "#778294"]
};

function findNode(root: TreemapNodeDatum, id: string): TreemapNodeDatum | null {
  if (root.id === id) {
    return root;
  }
  for (const child of root.children ?? []) {
    const found = findNode(child, id);
    if (found) {
      return found;
    }
  }
  return null;
}

function ancestorChain(root: TreemapNodeDatum, id: string): TreemapNodeDatum[] {
  const chain: TreemapNodeDatum[] = [];
  const walk = (node: TreemapNodeDatum, path: TreemapNodeDatum[]): boolean => {
    const next = [...path, node];
    if (node.id === id) {
      chain.push(...next);
      return true;
    }
    return (node.children ?? []).some((child) => walk(child, next));
  };
  walk(root, []);
  return chain;
}

/**
 * 只读体量 treemap：d3-hierarchy squarify 布局 + canvas 绘制。
 * 目录可下钻（仅到报告已有的数据深度），残差矩形不可下钻，卡片列表仍保留全部明细。
 */
export function TreemapView({ data }: TreemapViewProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [size, setSize] = useState({ width: 720, height: 320 });
  const [focusId, setFocusId] = useState(data.id);
  const [hovered, setHovered] = useState<LayoutNode | null>(null);

  // 焦点校准为派生状态：报告切换后 focus 可能指向旧路径，渲染时回退到根。
  const effectiveFocusId = useMemo(
    () => (findNode(data, focusId) ? focusId : data.id),
    [data, focusId]
  );

  useEffect(() => {
    const element = containerRef.current;
    if (!element) {
      return;
    }
    const observer = new ResizeObserver((entries) => {
      const rect = entries[0]?.contentRect;
      if (rect) {
        setSize({
          width: Math.max(320, Math.floor(rect.width)),
          height: Math.max(220, Math.floor(rect.height))
        });
      }
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const layout = useMemo<LayoutNode[]>(() => {
    const focusNode = findNode(data, effectiveFocusId) ?? data;
    const children = focusNode.children ?? [];
    if (children.length === 0) {
      return [];
    }
    const wrapper: TreemapNodeDatum = {
      id: `${focusNode.id}::wrap`,
      name: focusNode.name,
      sizeGb: focusNode.sizeGb,
      kind: focusNode.kind,
      children
    };
    const root = hierarchy<TreemapNodeDatum>(wrapper, (node) => node.children)
      .sum((node) => (node.children && node.children.length > 0 ? 0 : node.sizeGb))
      .sort((left, right) => (right.value ?? 0) - (left.value ?? 0));
    const layoutRoot = treemap<TreemapNodeDatum>()
      .size([size.width, size.height])
      .paddingInner(2)
      .paddingOuter(2)
      .round(true)
      .tile(treemapSquarify)(root) as HierarchyRectangularNode<TreemapNodeDatum>;
    // 取焦点的直接子节点（depth 1 全体，含目录节点本身）；
    // leaves() 会漏掉有孩子的目录，真实报告下矩形几乎全空。
    return (layoutRoot.children ?? []).map((node) => ({
      x0: node.x0,
      y0: node.y0,
      x1: node.x1,
      y1: node.y1,
      depth: node.depth,
      datum: node.data
    }));
  }, [data, effectiveFocusId, size]);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) {
      return;
    }
    const ratio = window.devicePixelRatio || 1;
    canvas.width = size.width * ratio;
    canvas.height = size.height * ratio;
    const context = canvas.getContext("2d");
    if (!context) {
      return;
    }
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.clearRect(0, 0, size.width, size.height);

    for (const node of layout) {
      const width = node.x1 - node.x0;
      const height = node.y1 - node.y0;
      const isHovered = hovered?.datum.id === node.datum.id;
      const [fill] = KIND_COLORS[node.datum.kind];
      context.fillStyle = isHovered ? "#f4a261" : fill;
      context.fillRect(node.x0, node.y0, width, height);
      context.strokeStyle = "#0f1c24";
      context.lineWidth = 1;
      context.strokeRect(node.x0 + 0.5, node.y0 + 0.5, width - 1, height - 1);

      if (width > 56 && height > 30) {
        context.fillStyle = "#f7f7f2";
        context.font = "12px sans-serif";
        context.fillText(
          node.datum.name.slice(0, Math.floor(width / 8)),
          node.x0 + 6,
          node.y0 + 18
        );
        if (height > 46) {
          context.font = "11px sans-serif";
          context.fillStyle = "#d8e2dc";
          context.fillText(formatSize(node.datum.sizeGb), node.x0 + 6, node.y0 + 34);
        }
      }
    }
  }, [hovered, layout, size]);

  useEffect(() => {
    draw();
  }, [draw]);

  const hitTest = useCallback(
    (event: React.MouseEvent<HTMLCanvasElement>) => {
      const canvas = canvasRef.current;
      if (!canvas) {
        return null;
      }
      const rect = canvas.getBoundingClientRect();
      const x = event.clientX - rect.left;
      const y = event.clientY - rect.top;
      return (
        layout.find((node) => x >= node.x0 && x <= node.x1 && y >= node.y0 && y <= node.y1) ?? null
      );
    },
    [layout]
  );

  const chain = useMemo(() => ancestorChain(data, effectiveFocusId), [data, effectiveFocusId]);
  const focusNode = findNode(data, effectiveFocusId) ?? data;

  return (
    <div className="treemap-panel" ref={containerRef}>
      <div className="treemap-toolbar">
        <nav className="treemap-crumbs" aria-label="treemap 下钻路径">
          {chain.map((node, index) => (
            <span key={node.id}>
              {index > 0 ? <span aria-hidden> / </span> : null}
              <button type="button" onClick={() => setFocusId(node.id)}>
                {node.name}
              </button>
            </span>
          ))}
        </nav>
        <span className="treemap-focus-size">
          {focusNode.name}：{formatSize(focusNode.sizeGb)}
        </span>
      </div>
      <canvas
        ref={canvasRef}
        style={{ width: size.width, height: size.height }}
        role="img"
        aria-label={`只读体量图：${focusNode.name}，共 ${layout.length} 个矩形。明细见下方卡片列表。`}
        onMouseMove={(event) => setHovered(hitTest(event))}
        onMouseLeave={() => setHovered(null)}
        onClick={(event) => {
          const node = hitTest(event);
          if (node && node.datum.kind !== "residual" && node.datum.children?.length) {
            setFocusId(node.datum.id);
          }
        }}
      />
      {hovered ? (
        <div className="treemap-tooltip" role="status">
          <strong>{hovered.datum.name}</strong>
          <span>{treemapNodeDetail(hovered.datum)}</span>
          {hovered.datum.children?.length ? <em>点击可下钻</em> : null}
        </div>
      ) : null}
      <p className="treemap-hint">
        只读聚合视图：面积按体量，深色为目录、绿色为文件、灰色为「其他」聚合项。下钻深度受报告数据限制，不做实时磁盘访问。
      </p>
    </div>
  );
}
