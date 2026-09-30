import js from '@eslint/js';
import skipFormatting from '@vue/eslint-config-prettier/skip-formatting';
import {
  configureVueProject,
  defineConfigWithVueTs,
  vueTsConfigs,
} from '@vue/eslint-config-typescript';
import pluginVue from 'eslint-plugin-vue';

// Type-aware linting needs to know which files belong to the Vue project.
configureVueProject({ rootDir: import.meta.dirname, scriptLangs: ['ts'] });

export default defineConfigWithVueTs(
  {
    name: 'localme/ignores',
    ignores: ['dist/**', 'node_modules/**', 'coverage/**', 'src-tauri/**', 'tools/**', 'docs/**'],
  },
  {
    name: 'localme/files',
    files: ['**/*.ts', '**/*.vue'],
  },
  js.configs.recommended,
  pluginVue.configs['flat/recommended'],
  vueTsConfigs.strictTypeChecked,
  skipFormatting,
  {
    name: 'localme/rules',
    rules: {
      // The design system is a token system plus explicit components; ad-hoc elements with
      // utility classes would be the start of a second one.
      'vue/component-api-style': ['error', ['script-setup']],
      'vue/block-lang': ['error', { script: { lang: 'ts' } }],
      'vue/multi-word-component-names': ['error', { ignores: ['App'] }],
      'vue/no-v-html': 'error',
      'vue/define-macros-order': [
        'error',
        { order: ['defineOptions', 'defineProps', 'defineEmits', 'defineSlots'] },
      ],
      'vue/no-unused-properties': 'off',
      // Optional props declared with TypeScript types are `undefined` when absent, so a
      // `withDefaults` entry saying exactly that would be noise without changing behaviour.
      // The rule exists for the options API, where an absent prop had to be spelled out.
      'vue/require-default-prop': 'off',

      // Message bodies and nicknames are rendered as text, never as markup.
      'no-restricted-syntax': [
        'error',
        {
          selector: "JSXAttribute[name.name='innerHTML']",
          message: 'Rendering HTML from message content is a stored-XSS vector.',
        },
        {
          selector: "CallExpression[callee.property.name='insertAdjacentHTML']",
          message: 'Rendering HTML from message content is a stored-XSS vector.',
        },
        {
          selector: "AssignmentExpression[left.property.name='innerHTML']",
          message: 'Rendering HTML from message content is a stored-XSS vector.',
        },
      ],

      'consistent-return': 'error',
      eqeqeq: ['error', 'always'],
      'no-console': ['error', { allow: ['warn', 'error'] }],
      'no-implicit-coercion': 'error',
      'object-shorthand': 'error',
      'prefer-template': 'error',

      // `import type` for type-only imports keeps the emitted modules honest under
      // `verbatimModuleSyntax`.
      '@typescript-eslint/consistent-type-imports': ['error', { prefer: 'type-imports' }],
      '@typescript-eslint/explicit-function-return-type': [
        'error',
        { allowExpressions: true, allowTypedFunctionExpressions: true },
      ],
      '@typescript-eslint/no-unnecessary-condition': 'off',
      '@typescript-eslint/restrict-template-expressions': ['error', { allowNumber: true }],
    },
  },
);
