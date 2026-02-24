import React from 'react'
import './Card.css'

interface CardProps {
  children: React.ReactNode
  className?: string
  onClick?: () => void
  variant?: 'default' | 'elevated' | 'filled' | 'outlined'
  padding?: 'sm' | 'md' | 'lg' | 'xl'
  interactive?: boolean
}

const Card: React.FC<CardProps> = ({
  children,
  className = '',
  onClick,
  variant = 'default',
  padding = 'lg',
  interactive = false,
}) => {
  return (
    <div
      className={`card card--${variant} card--padding-${padding} ${interactive ? 'card--interactive' : ''} ${className}`}
      onClick={onClick}
      role={interactive ? 'button' : undefined}
      tabIndex={interactive ? 0 : undefined}
    >
      {children}
    </div>
  )
}

export default Card
