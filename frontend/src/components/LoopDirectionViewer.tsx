import React, { useState } from "react";
import { LoopSignal } from "../types";

interface LoopDirectionViewerProps {
  signals: LoopSignal[];
}

/**
 * SVG 기반 루프 오작동 순환 디렉션 뷰어 컴포넌트
 * 
 * 핑퐁 루프나 자가 반복 루프 시그널이 감지되었을 때 SVG를 이용해 순환 다이어그램을 렌더링합니다.
 */
export function LoopDirectionViewer({ signals }: LoopDirectionViewerProps) {
  const pingPong = signals.find((s) => s.signal_type === "ping_pong");
  const selfLoop = signals.find((s) => s.signal_type === "repeated_call");

  const [hoveredTool, setHoveredTool] = useState<{ name: string; x: number; y: number } | null>(null);

  const truncateToolName = (name: string, maxLen = 14) => {
    if (name.length <= maxLen) return name;
    return name.substring(0, maxLen) + "...";
  };

  const handleMouseMove = (e: React.MouseEvent, name: string) => {
    const container = e.currentTarget.closest(".loop-viewer-container");
    if (!container) return;
    const rect = container.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    setHoveredTool({ name, x, y });
  };

  const handleMouseLeave = () => {
    setHoveredTool(null);
  };

  if (pingPong) {
    const evidence = pingPong.evidence;
    const parts = evidence.split(",").map(p => p.trim());
    let toolA = "Tool A";
    let toolB = "Tool B";
    let cycles = "3";
    for (const part of parts) {
      if (part.startsWith("tool_A=")) toolA = part.substring(7);
      if (part.startsWith("tool_B=")) toolB = part.substring(7);
      if (part.startsWith("cycles=")) cycles = part.substring(7);
    }

    return (
      <div className="loop-viewer-container" style={{ position: "relative" }}>
        <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "0.5rem" }}>
          <span style={{ fontSize: "0.85rem", fontWeight: 700, color: "var(--neon-red)" }}>🔄 핑퐁 순환 흐름도</span>
          <span className="badge-cycles">{cycles} Cycles</span>
        </div>
        <svg viewBox="0 0 400 140" className="loop-svg">
          <defs>
            <marker id="arrow-red-right" markerWidth="6" markerHeight="6" refX="5" refY="3" orient="auto">
              <path d="M0,0 L6,3 L0,6 Z" fill="var(--neon-red)" />
            </marker>
            <marker id="arrow-red-left" markerWidth="6" markerHeight="6" refX="1" refY="3" orient="auto">
              <path d="M6,0 L0,3 L6,6 Z" fill="var(--neon-red)" />
            </marker>
          </defs>
          
          {/* A -> B 위로 휘는 곡선 */}
          <path d="M 130 55 Q 200 15 270 55" className="loop-line dash-flow-red" markerEnd="url(#arrow-red-right)" />
          
          {/* B -> A 아래로 휘는 곡선 */}
          <path d="M 270 85 Q 200 125 130 85" className="loop-line dash-flow-red" markerEnd="url(#arrow-red-left)" />

          {/* Node A */}
          <g 
            style={{ cursor: "help" }}
            onMouseMove={(e) => handleMouseMove(e, toolA)}
            onMouseLeave={handleMouseLeave}
          >
            <circle cx="95" cy="70" r="30" className="loop-node-circle" />
            <text x="95" y="73" textAnchor="middle" className="loop-node-text" style={{ fontSize: "10px" }}>{truncateToolName(toolA)}</text>
          </g>

          {/* Node B */}
          <g 
            style={{ cursor: "help" }}
            onMouseMove={(e) => handleMouseMove(e, toolB)}
            onMouseLeave={handleMouseLeave}
          >
            <circle cx="305" cy="70" r="30" className="loop-node-circle" />
            <text x="305" y="73" textAnchor="middle" className="loop-node-text" style={{ fontSize: "10px" }}>{truncateToolName(toolB)}</text>
          </g>
        </svg>

        {hoveredTool && (
          <div 
            className="tooltip-text" 
            style={{ 
              visibility: "visible", 
              opacity: 1, 
              position: "absolute", 
              left: `${hoveredTool.x}px`, 
              top: `${hoveredTool.y}px`, 
              transform: "translate(-50%, -105%)", 
              width: "250px", 
              height: "auto",
              minHeight: "fit-content",
              padding: "0.65rem 0.85rem",
              lineHeight: "1.45",
              boxSizing: "border-box",
              pointerEvents: "none",
              wordBreak: "break-all",
              whiteSpace: "normal",
              zIndex: 999
            }}
          >
            <b>도구명 전체 식별자</b>:<br/>
            {hoveredTool.name}
          </div>
        )}
      </div>
    );
  }

  if (selfLoop) {
    const evidence = selfLoop.evidence;
    const parts = evidence.split(",").map(p => p.trim());
    let toolName = "Tool";
    let count = "3";
    for (const part of parts) {
      if (part.startsWith("tool_name=")) toolName = part.substring(10);
      if (part.startsWith("count=")) count = part.substring(6);
    }

    return (
      <div className="loop-viewer-container" style={{ position: "relative" }}>
        <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "0.5rem" }}>
          <span style={{ fontSize: "0.85rem", fontWeight: 700, color: "var(--neon-red)" }}>🔁 자가 순환 루프</span>
          <span className="badge-cycles">{count} Reps</span>
        </div>
        <svg viewBox="0 0 400 140" className="loop-svg">
          <defs>
            <marker id="arrow-red-self" markerWidth="6" markerHeight="6" refX="5" refY="3" orient="auto">
              <path d="M0,0 L6,3 L0,6 Z" fill="var(--neon-red)" />
            </marker>
          </defs>
          
          {/* Self feedback loop path */}
          <path d="M 185 50 C 130 -10, 270 -10, 215 50" className="loop-line dash-flow-red" markerEnd="url(#arrow-red-self)" />

          <g 
            style={{ cursor: "help" }}
            onMouseMove={(e) => handleMouseMove(e, toolName)}
            onMouseLeave={handleMouseLeave}
          >
            <circle cx="200" cy="80" r="30" className="loop-node-circle" />
            <text x="200" y="83" textAnchor="middle" className="loop-node-text" style={{ fontSize: "10px" }}>{truncateToolName(toolName)}</text>
          </g>
        </svg>

        {hoveredTool && (
          <div 
            className="tooltip-text" 
            style={{ 
              visibility: "visible", 
              opacity: 1, 
              position: "absolute", 
              left: `${hoveredTool.x}px`, 
              top: `${hoveredTool.y}px`, 
              transform: "translate(-50%, -105%)", 
              width: "250px", 
              height: "auto",
              minHeight: "fit-content",
              padding: "0.65rem 0.85rem",
              lineHeight: "1.45",
              boxSizing: "border-box",
              pointerEvents: "none",
              wordBreak: "break-all",
              whiteSpace: "normal",
              zIndex: 999
            }}
          >
            <b>도구명 전체 식별자</b>:<br/>
            {hoveredTool.name}
          </div>
        )}
      </div>
    );
  }

  return null;
}
export default LoopDirectionViewer;
