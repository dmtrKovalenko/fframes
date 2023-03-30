import * as React from "react";

export const PlayBackIcon = ({ backward, text, ...props }) => (
  <svg
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    viewBox="0 0 57 57"
    {...props}
  >
    <path
      d="M49.875 30.875A21.375 21.375 0 1 1 28.5 9.5h17.813m0 0-4.75-4.75m4.75 4.75-4.75 4.75"
      stroke="currentColor"
      transform-origin="50% 50%"
      style={backward ? { transform: "rotateY(180deg)" } : undefined}
      strokeWidth={3.563}
      strokeLinecap="round"
      strokeLinejoin="round"
    />

    <text
      x={29}
      textAnchor="middle"
      y={37}
      fontFamily="ff_internal_Virgil"
      fill="currentColor"
      fontSize={text.length === 1 ? 24 : 20}
    >
      {text}
    </text>
  </svg>
);
