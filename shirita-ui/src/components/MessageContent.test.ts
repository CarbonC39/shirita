import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import MessageContent from './MessageContent.vue'

describe('MessageContent', () => {
  it('folds a closed <think> block into a collapsed details, answer stays outside', () => {
    const w = mount(MessageContent, { props: { text: '<think>secret reasoning</think>The answer' } })
    const details = w.find('details')
    expect(details.exists()).toBe(true)
    expect(details.attributes('open')).toBeUndefined() // collapsed by default
    expect(w.find('summary').text()).toBe('Thoughts')
    expect(details.text()).toContain('secret reasoning')
    expect(w.text()).toContain('The answer')
  })

  it('keeps an unclosed (still-streaming) think block open', () => {
    const w = mount(MessageContent, { props: { text: '<think>thinking out loud' } })
    expect(w.find('details').attributes('open')).toBeDefined()
  })

  it('renders plain text without any details block', () => {
    const w = mount(MessageContent, { props: { text: 'just an answer' } })
    expect(w.find('details').exists()).toBe(false)
    expect(w.text()).toContain('just an answer')
  })

  it('renders markdown inside both the think block and the answer', () => {
    const w = mount(MessageContent, { props: { text: '<think>**why**</think>**done**' } })
    expect(w.findAll('strong').length).toBe(2)
  })
})
