import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, waitFor } from '@testing-library/vue'
import ClipboardImage from './ClipboardImage.vue'
const mocks = vi.hoisted(() => ({ invoke: vi.fn(), reportImageReadable: vi.fn(), reportImageAccessDenied: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }))
vi.mock('$lib/stores/clipboardStore', () => ({ useClipboardStore: () => mocks }))
vi.mock('$lib/i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
afterEach(() => { cleanup(); mocks.invoke.mockReset() })
const src = 'data:image/png;base64,cHJldmlldw=='

describe('clipboard images from application Containers', () => {
  it('requests a thumbnail by ID instead of loading an out-of-scope file URL', async () => {
    mocks.invoke.mockResolvedValue(src)
    const view = render(ClipboardImage, { props: { id: 'qq-image', content: '/Users/me/Library/Containers/QQ/picture.png' } })
    await waitFor(() => expect(view.getByRole('img').getAttribute('src')).toBe(src))
    expect(mocks.invoke).toHaveBeenCalledWith('get_clipboard_image_thumbnail', { id: 'qq-image' })
    expect(view.container.innerHTML).not.toContain('asset.localhost')
  })
  it('lets the user retry an inaccessible image', async () => {
    mocks.invoke.mockRejectedValueOnce(new Error('CLIPBOARD_IMAGE_ACCESS_DENIED')).mockResolvedValueOnce(src)
    const view = render(ClipboardImage, { props: { id: 'qq-image', content: '/container/picture.png' } })
    await waitFor(() => expect(view.getByRole('button')).toBeTruthy())
    expect(mocks.reportImageAccessDenied).toHaveBeenCalledWith('qq-image')
    await fireEvent.click(view.getByRole('button'))
    await waitFor(() => expect(view.getByRole('img').getAttribute('src')).toBe(src))
    expect(mocks.invoke).toHaveBeenCalledTimes(2)
  })
  it('does not replace a new entry with a stale thumbnail response', async () => {
    let resolveOld!: (value: string) => void
    mocks.invoke.mockImplementationOnce(() => new Promise<string>((resolve) => { resolveOld = resolve })).mockResolvedValueOnce(src)
    const view = render(ClipboardImage, { props: { id: 'old', content: '/container/old.png' } })
    await view.rerender({ id: 'new', content: '/container/new.png' })
    await waitFor(() => expect(view.getByRole('img').getAttribute('src')).toBe(src))
    resolveOld('data:image/png;base64,b2xk')
    await Promise.resolve()
    expect(view.getByRole('img').getAttribute('src')).toBe(src)
  })
})
