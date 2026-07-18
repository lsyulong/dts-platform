type Props = {
  size?: 'sm' | 'md' | 'lg';
};

const SIZE_CLASSES = {
  sm: 'h-6 w-6',
  md: 'h-8 w-8',
  lg: 'h-12 w-12',
} as const;

export function LoadingSpinner({ size = 'md' }: Props) {
  return (
    <div className="flex justify-center py-10">
      <div
        className={`${SIZE_CLASSES[size]} animate-spin rounded-full border-2 border-[var(--color-border-hover)] border-t-blue-400`}
      />
    </div>
  );
}
