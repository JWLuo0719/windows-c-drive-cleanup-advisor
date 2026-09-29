import js from "@eslint/js";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";

// 前端工程门槛（Phase 1 Stage 5）：lint 失败即门禁失败。
// 规则选择原则：能拦住真实回归（hooks 依赖、未用代码、危险语法），
// 不引入与现有代码风格无关的格式噪声（格式由 Prettier 负责）。
export default tseslint.config(
  {
    ignores: ["dist/**", "src-tauri/target/**", "node_modules/**", "coverage/**"]
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["src/**/*.{ts,tsx}", "vite.config.ts"],
    plugins: {
      "react-hooks": reactHooks
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      // 项目纪律：UI 阶段判断禁止解析消息文案（进度协议消费端约束）。
      "no-restricted-syntax": [
        "error",
        {
          selector: "MemberExpression[property.name='includes'][object.name='message']",
          message: "禁止解析 message 文案推断扫描阶段，请只消费 percent（进度协议）。"
        }
      ]
    }
  }
);
