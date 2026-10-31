import { GrillPage } from "~/modules/profile";
import { SpendLine } from "~/modules/sources";

export interface ProfileGrillProps {
  /** The position open (`/profile/grill/$position`); none shows the roles alone. */
  position: number | undefined;
  onPosition: (position: number) => void;
  onBack: () => void;
}

/**
 * `/profile/grill`: Grill me, a deep interview about one role at a time, in
 * the app frame. Opening it asks nothing of the model; the spend line says
 * what the month has cost.
 */
export function ProfileGrill({ position, onPosition, onBack }: ProfileGrillProps) {
  return (
    <GrillPage
      position={position}
      onPosition={onPosition}
      onBack={onBack}
      spend={<SpendLine testId="grill-spend" />}
    />
  );
}
