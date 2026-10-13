import { cn } from '@/lib/utils';

function Skeleton({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      data-slot="skeleton"
      className={cn('animate-pulse rounded-md bg-[var(--color-bg-tertiary)]', className)}
      {...props}
    />
  );
}

export { Skeleton };
