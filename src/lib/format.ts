import type { Metered, TestError } from './model.ts'

export function formatMbps(value: number): string {
  if (!Number.isFinite(value) || value < 0) return '–'
  if (value >= 10) return String(Math.round(value))
  if (value >= 1) return value.toFixed(1)
  return value.toFixed(2)
}

export function formatMs(value: number): string {
  if (!Number.isFinite(value) || value < 0) return '–'
  return value < 1 ? '<1' : String(Math.round(value))
}

export function formatMb(bytes: number): string {
  const mb = bytes / 1_000_000
  if (mb < 1) return '<1 MB'
  return `${mb < 10 ? mb.toFixed(1) : Math.round(mb)} MB`
}

export function relativeAge(at: number, now: number): string {
  const seconds = Math.max(0, Math.floor((now - at) / 1000))
  if (seconds < 60) return 'just now'
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes} min ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours} h ago`
  const days = Math.floor(hours / 24)
  return days === 1 ? '1 day ago' : `${days} days ago`
}

export function clock(at: number): string {
  return new Date(at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

const cities: Record<string, string> = {
  AMS: 'Amsterdam', ARN: 'Stockholm', ATH: 'Athens', ATL: 'Atlanta', BCN: 'Barcelona', BEG: 'Belgrade', BKK: 'Bangkok', BNE: 'Brisbane',
  BOG: 'Bogotá', BOM: 'Mumbai', BOS: 'Boston', BRU: 'Brussels', BUD: 'Budapest', CAI: 'Cairo', CDG: 'Paris', CGK: 'Jakarta', CPH: 'Copenhagen',
  CPT: 'Cape Town', DEL: 'New Delhi', DEN: 'Denver', DFW: 'Dallas', DME: 'Moscow', DOH: 'Doha', DUB: 'Dublin', DUS: 'Düsseldorf', DXB: 'Dubai',
  EWR: 'Newark', EZE: 'Buenos Aires', FCO: 'Rome', FRA: 'Frankfurt', GIG: 'Rio de Janeiro', GRU: 'São Paulo', HAM: 'Hamburg', HEL: 'Helsinki',
  HKG: 'Hong Kong', IAD: 'Ashburn', ICN: 'Seoul', IST: 'Istanbul', JNB: 'Johannesburg', KIX: 'Osaka', KUL: 'Kuala Lumpur', LAX: 'Los Angeles',
  LHR: 'London', LIM: 'Lima', LIS: 'Lisbon', MAD: 'Madrid', MAN: 'Manchester', MEL: 'Melbourne', MEX: 'Mexico City', MIA: 'Miami',
  MRS: 'Marseille', MSP: 'Minneapolis', MUC: 'Munich', MXP: 'Milan', NBO: 'Nairobi', NRT: 'Tokyo', ORD: 'Chicago', OSL: 'Oslo', OTP: 'Bucharest',
  PDX: 'Portland', PHX: 'Phoenix', PRG: 'Prague', SCL: 'Santiago', SEA: 'Seattle', SFO: 'San Francisco', SIN: 'Singapore', SJC: 'San Jose',
  SOF: 'Sofia', SYD: 'Sydney', TLV: 'Tel Aviv', TPE: 'Taipei', VIE: 'Vienna', WAW: 'Warsaw', YUL: 'Montréal', YVR: 'Vancouver', YYZ: 'Toronto',
  ZRH: 'Zurich', ALA: 'Almaty', TAS: 'Tashkent', TBS: 'Tbilisi', EVN: 'Yerevan', GYD: 'Baku', KBP: 'Kyiv', LED: 'Saint Petersburg', RIX: 'Riga',
  TLL: 'Tallinn', VNO: 'Vilnius', KHI: 'Karachi', ISB: 'Islamabad', LHE: 'Lahore', DAC: 'Dhaka', CMB: 'Colombo', MNL: 'Manila', SGN: 'Ho Chi Minh City',
  HAN: 'Hanoi', AKL: 'Auckland', PER: 'Perth', LOS: 'Lagos', ACC: 'Accra', CMN: 'Casablanca', TUN: 'Tunis', RUH: 'Riyadh', JED: 'Jeddah',
  KWI: 'Kuwait City', BAH: 'Manama', MCT: 'Muscat', AMM: 'Amman', BEY: 'Beirut',
}

export function server(provider: string, colo: string | null): string {
  if (!colo) return provider
  return `${provider}, ${cities[colo] ?? colo}`
}

export function estimateText(estimate: { upTo: boolean; mb: number }): string {
  return estimate.upTo ? `Uses up to ${estimate.mb} MB` : `Uses about ${estimate.mb} MB`
}

export const errorCopy: Record<TestError, { title: string; detail: string }> = {
  offline: { title: 'No internet', detail: 'Weir couldn’t reach Cloudflare. Check your connection.' },
  refused: { title: 'Cloudflare refused the test', detail: 'Try again in a few minutes.' },
  serverError: { title: 'Cloudflare server error', detail: 'Try again in a few minutes.' },
  stalled: { title: 'Connection stalled', detail: 'No data arrived for 3 seconds.' },
  tooShort: { title: 'Test too short to measure', detail: 'The connection didn’t settle long enough for a fair number.' },
  dropped: { title: 'Connection kept dropping', detail: 'More than half the connections failed.' },
  networkChanged: { title: 'Network changed during the test', detail: 'Test again on the new network.' },
  cancelled: { title: 'Cancelled', detail: 'No result was saved.' },
}

export const meteredCopy: Record<Metered, string> = {
  hotspot: 'You’re on a hotspot. Test anyway?',
  lowData: 'Low Data Mode is on. Test anyway?',
  metered: 'This connection is metered. Test anyway?',
  roaming: 'You’re roaming. Test anyway?',
  overLimit: 'You’re over your data limit. Test anyway?',
}

export const meteredLabel: Record<Metered, string> = {
  hotspot: 'Hotspot',
  lowData: 'Low Data Mode',
  metered: 'Metered connection',
  roaming: 'Roaming',
  overLimit: 'Over data limit',
}
