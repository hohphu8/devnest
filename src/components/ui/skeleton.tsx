interface SkeletonProps {
  height?: number;
}

export function Skeleton({ height = 40 }: SkeletonProps) {
  return <div className="skeleton" style={{ height }} />;
}
