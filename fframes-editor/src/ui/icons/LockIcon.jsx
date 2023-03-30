import * as React from "react";

export const LockIcon = (props) => (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" {...props}>
    <path
      d="M336 208v-95a80 80 0 0 0-160 0v95"
      fill="none"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={32}
    />
    <rect
      x={96}
      y={208}
      width={320}
      height={272}
      rx={48}
      ry={48}
      fill="none"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={32}
    />
  </svg>
);
