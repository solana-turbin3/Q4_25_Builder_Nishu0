"use client"

import type React from "react"
import { motion } from "framer-motion"
import { useState, useEffect } from "react"
import { Badge } from "@/components/ui/badge"
import { Sparkles } from "lucide-react"
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"

export default function Hero() {
  const [mounted, setMounted] = useState(false)
  const [email, setEmail] = useState("")
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [isSubmitted, setIsSubmitted] = useState(false)

  useEffect(() => {
    setMounted(true)
  }, [])

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setIsSubmitting(true)
    // Simulate API call
    await new Promise((resolve) => setTimeout(resolve, 1000))
    setIsSubmitting(false)
    setIsSubmitted(true)
    setEmail("")
    // Reset success message after 3 seconds
    setTimeout(() => setIsSubmitted(false), 3000)
  }

  if (!mounted) {
    return null
  }

  return (
    <>
      <section className="relative overflow-hidden min-h-screen flex flex-col">
        <div className="container mx-auto px-4 py-24 sm:py-32 relative z-10 flex-1 flex flex-col">
          <div className="mx-auto max-w-4xl text-center flex-1 flex flex-col justify-center">
            {/* Badge */}
            <motion.div
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.5 }}
              className="mb-8"
            >
              <Badge variant="secondary" className="inline-flex items-center gap-2 px-4 py-2 text-sm">
                <Sparkles className="h-4 w-4" />
                Power Perpetuals on Solana
              </Badge>
            </motion.div>

            {/* Main Heading */}
            <motion.div
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.5, delay: 0.1 }}
              className="mb-8"
            >
              <h1 id="main-title" className="text-4xl font-bold tracking-tight text-foreground sm:text-6xl lg:text-7xl">
                <strong>Flash</strong> <br />
                <em className="italic">Power Perps</em> <br />
              </h1>
            </motion.div>

            {/* Description */}
            <motion.p
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.5, delay: 0.2 }}
              className="mx-auto mb-12 max-w-2xl text-lg text-muted-foreground"
            >
              Power perpetuals give traders squared exposure to crypto assets without liquidation risk. On Solana, fees are 1000x cheaper than Ethereum, making it accessible to users with $1k instead of needing $50k+ positions.
            </motion.p>

            <motion.div
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.5, delay: 0.3 }}
              className="flex flex-col items-center gap-6"
            >
              {/* Email Registration Form */}
              <form onSubmit={handleSubmit} className="w-full max-w-md">
                <div className="flex flex-col sm:flex-row gap-3 items-center">
                  <Input
                    type="email"
                    placeholder="Enter your email"
                    value={email}
                    onChange={(e) => setEmail(e.target.value)}
                    className="flex-1 h-12 bg-background/50 border-border text-foreground placeholder:text-muted-foreground focus:border-primary"
                    required
                    disabled={isSubmitting || isSubmitted}
                  />
                  <Button
                    type="submit"
                    disabled={isSubmitting || isSubmitted}
                    className="h-12 px-8 bg-primary hover:bg-primary/90 text-primary-foreground font-medium whitespace-nowrap"
                  >
                    {isSubmitting ? "Joining..." : isSubmitted ? "Joined!" : "Join Waitlist"}
                  </Button>
                </div>
                {isSubmitted && (
                  <motion.p
                    initial={{ opacity: 0, y: -10 }}
                    animate={{ opacity: 1, y: 0 }}
                    className="mt-3 text-sm text-green-400 text-center"
                  >
                    ✓ You've been added to the waitlist!
                  </motion.p>
                )}
              </form>

              {/* Waitlist Count */}
              <motion.div
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                transition={{ duration: 0.5, delay: 0.5 }}
                className="text-center"
              >
                <p className="text-sm text-muted-foreground">
                  <span className="font-semibold text-foreground">100+</span> joined the waitlist
                </p>
              </motion.div>
            </motion.div>
          </div>

        </div>
      </section>
    </>
  )
}
