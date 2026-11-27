import type { CSSProperties } from 'react';
import type { HarnessId, ThemeId } from '../types';

export const HARNESS_META: Record<
  HarnessId,
  { name: string; short: string; colorVar: string; models: string[] }
> = {
  'agy-cli': {
    name: 'AGY CLI',
    short: 'AGY',
    colorVar: 'var(--accent-blue)',
    models: [
      'gemini-4-hydrogen',
      'gemini-3.8-flash',
      'claude-sonnet-4-6',
      'claude-opus-4-6-thinking',
      'gpt-oss-120b',
    ],
  },
  'claude-code': {
    name: 'Claude Code',
    short: 'Claude',
    colorVar: 'var(--accent-yellow)',
    models: ['claude-sonnet-4-6', 'claude-opus-4-6', 'claude-haiku-4-5'],
  },
  'kiro-cli': {
    name: 'Kiro CLI',
    short: 'Kiro',
    colorVar: 'var(--accent-green)',
    models: [
      'auto',
      'claude-sonnet-5.5',
      'claude-opus-5.5',
      'claude-haiku-5.5',
      'gpt-5.6-sol',
      'gpt-5.6-terra',
      'deepseek-3.2',
      'qwen3-coder-next',
    ],
  },
  sakana: {
    name: 'Sakana Master',
    short: 'Sakana',
    colorVar: 'var(--text-main)',
    models: [
      'gemini-4-carbon',
      'gemini-4-hydrogen',
      'gemini-4-argon',
      'gemini-4-neon',
      'gemini-3.8-flash',
      'gemini-3.5-flash',
      'gemini-3.1-pro',
      'gemini-2.5-pro',
      'gemini-flash',
      'auto',
    ],
  },
};

export const THEME_META: { id: ThemeId; label: string; kind: 'Light' | 'Dark' }[] = [
  { id: 'paper', label: 'Paper', kind: 'Light' },
  { id: 'noir', label: 'Noir OLED', kind: 'Dark' },
  { id: 'slate', label: 'Linear Slate', kind: 'Dark' },
  { id: 'sand', label: 'Warm Sand', kind: 'Light' },
];

export function HarnessIcon({
  harness,
  size = 13,
  className = '',
  style,
}: {
  harness: HarnessId | 'all';
  size?: number;
  className?: string;
  style?: CSSProperties;
}) {
  if (harness === 'all') {
    return (
      <svg
        width={size}
        height={size}
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        className={`shrink-0 ${className}`}
        style={style}
      >
        <rect x="3" y="3" width="7" height="7" rx="1.5" />
        <rect x="14" y="3" width="7" height="7" rx="1.5" />
        <rect x="3" y="14" width="7" height="7" rx="1.5" />
        <rect x="14" y="14" width="7" height="7" rx="1.5" />
      </svg>
    );
  }

  const color = HARNESS_META[harness]?.colorVar ?? 'currentColor';
  const mergedStyle: CSSProperties = { color, ...style };

  if (harness === 'agy-cli') {
    return (
      <svg
        width={size}
        height={size}
        viewBox="0 0 24 24"
        fill="currentColor"
        className={`shrink-0 ${className}`}
        style={mergedStyle}
      >
        <path d="M19.94 20.59c1.09.82 2.73.27 1.23-1.23C16.67 15 17.62 3 12.03 3S7.39 15 2.89 19.36c-1.64 1.64.14 2.05 1.23 1.23c4.23-2.86 3.95-7.91 7.91-7.91s3.68 5.05 7.91 7.91" />
      </svg>
    );
  }

  if (harness === 'claude-code') {
    return (
      <svg
        width={size}
        height={size}
        viewBox="0 0 24 24"
        fill="currentColor"
        className={`shrink-0 ${className}`}
        style={mergedStyle}
      >
        <path d="m4.7144 15.9555 4.7174-2.6471.079-.2307-.079-.1275h-.2307l-.7893-.0486-2.6956-.0729-2.3375-.0971-2.2646-.1214-.5707-.1215-.5343-.7042.0546-.3522.4797-.3218.686.0608 1.5179.1032 2.2767.1578 1.6514.0972 2.4468.255h.3886l.0546-.1579-.1336-.0971-.1032-.0972L6.973 9.8356l-2.55-1.6879-1.3356-.9714-.7225-.4918-.3643-.4614-.1578-1.0078.6557-.7225.8803.0607.2246.0607.8925.686 1.9064 1.4754 2.4893 1.8336.3643.3035.1457-.1032.0182-.0728-.164-.2733-1.3539-2.4467-1.445-2.4893-.6435-1.032-.17-.6194c-.0607-.255-.1032-.4674-.1032-.7285L6.287.1335 6.6997 0l.9957.1336.419.3642.6192 1.4147 1.0018 2.2282 1.5543 3.0296.4553.8985.2429.8318.091.255h.1579v-.1457l.1275-1.706.2368-2.0947.2307-2.6957.0789-.7589.3764-.9107.7468-.4918.5828.2793.4797.686-.0668.4433-.2853 1.8517-.5586 2.9021-.3643 1.9429h.2125l.2429-.2429.9835-1.3053 1.6514-2.0643.7286-.8196.85-.9046.5464-.4311h1.0321l.759 1.1293-.34 1.1657-1.0625 1.3478-.8804 1.1414-1.2628 1.7-.7893 1.36.0729.1093.1882-.0183 2.8535-.607 1.5421-.2794 1.8396-.3157.8318.3886.091.3946-.3278.8075-1.967.4857-2.3072.4614-3.4364.8136-.0425.0304.0486.0607 1.5482.1457.6618.0364h1.621l3.0175.2247.7892.522.4736.6376-.079.4857-1.2142.6193-1.6393-.3886-3.825-.9107-1.3113-.3279h-.1822v.1093l1.0929 1.0686 2.0035 1.8092 2.5075 2.3314.1275.5768-.3218.4554-.34-.0486-2.2039-1.6575-.85-.7468-1.9246-1.621h-.1275v.17l.4432.6496 2.3436 3.5214.1214 1.0807-.17.3521-.6071.2125-.6679-.1214-1.3721-1.9246L14.38 17.959l-1.1414-1.9428-.1397.079-.674 7.2552-.3156.3703-.7286.2793-.6071-.4614-.3218-.7468.3218-1.4753.3886-1.9246.3157-1.53.2853-1.9004.17-.6314-.0121-.0425-.1397.0182-1.4328 1.9672-2.1796 2.9446-1.7243 1.8456-.4128.164-.7164-.3704.0667-.6618.4008-.5889 2.386-3.0357 1.4389-1.882.929-1.0868-.0062-.1579h-.0546l-6.3385 4.1164-1.1293.1457-.4857-.4554.0608-.7467.2307-.2429 1.9064-1.3114Z" />
      </svg>
    );
  }

  if (harness === 'kiro-cli') {
    return (
      <svg
        width={size}
        height={size}
        viewBox="0 0 256 312"
        fill="currentColor"
        className={`shrink-0 ${className}`}
        style={mergedStyle}
      >
        <path
          fillRule="evenodd"
          clipRule="evenodd"
          d="M49.375 241.177c-32.186 71.33 36.35 89.215 86.9 47.471c14.854 46.813 70.609 11.894 90.603-24.37c44.07-79.928 26.273-161.466 21.705-178.285c-31.328-114.698-187.91-114.812-214.86.585c-6.294 20.194-6.404 43.162-9.969 67.009c-1.797 12.025-3.064 19.7-7.717 32.348c-2.676 7.272-6.357 13.73-12.14 24.612c-8.982 16.904-5.192 49.53 41.103 32.61v.012zM142.393 135.634c-12.853 0-14.772-15.359-14.772-24.515c0-8.264 1.48-14.83 4.286-18.993c2.467-3.663 5.996-5.52 10.486-5.52s8.342 1.887 11.06 5.61c3.095 4.243 4.73 10.778 4.73 18.903c0 15.352-5.902 24.515-15.788 24.515zm52.875 0c-12.854 0-14.774-15.359-14.774-24.515c0-8.264 1.482-14.83 4.287-18.993c2.467-3.663 5.996-5.52 10.487-5.52s8.34 1.887 11.058 5.61c3.096 4.243 4.733 10.778 4.733 18.903c0 15.352-5.903 24.515-15.79 24.515z"
        />
      </svg>
    );
  }

  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.1"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`shrink-0 ${className}`}
      style={mergedStyle}
    >
      <path d="M12 2L2 7l10 5 10-5-10-5z" />
      <path d="M2 17l10 5 10-5" />
      <path d="M2 12l10 5 10-5" />
    </svg>
  );
}

export function DiscordIcon({
  size = 12,
  className = '',
  style,
}: {
  size?: number;
  className?: string;
  style?: CSSProperties;
}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="currentColor"
      className={`shrink-0 ${className}`}
      style={style}
    >
      <path d="M20.317 4.37a19.791 19.791 0 0 0-4.885-1.515.074.074 0 0 0-.079.037c-.21.375-.444.864-.608 1.25a18.27 18.27 0 0 0-5.487 0 12.64 12.64 0 0 0-.617-1.25.077.077 0 0 0-.079-.037A19.736 19.736 0 0 0 3.677 4.37a.07.07 0 0 0-.032.027C.533 9.046-.32 13.58.099 18.057a.082.082 0 0 0 .031.057 19.9 19.9 0 0 0 5.993 3.03.078.078 0 0 0 .084-.028c.462-.63.874-1.295 1.226-1.994.021-.041.001-.09-.041-.106a13.107 13.107 0 0 1-1.872-.892.077.077 0 0 1-.008-.128 10.2 10.2 0 0 0 .372-.292.074.074 0 0 1 .077-.01c3.928 1.793 8.18 1.793 12.062 0a.074.074 0 0 1 .078.01c.12.098.246.198.373.292a.077.077 0 0 1-.006.127 12.299 12.299 0 0 1-1.873.892.077.077 0 0 0-.041.107c.36.698.772 1.362 1.225 1.993a.076.076 0 0 0 .084.028 19.839 19.839 0 0 0 6.002-3.03.077.077 0 0 0 .032-.054c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 0 0-.031-.028zM8.02 15.33c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.956-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.956 2.418-2.157 2.418zm7.975 0c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.946 2.418-2.157 2.418z" />
    </svg>
  );
}
