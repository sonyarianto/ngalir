<script lang="ts">
  let { toastType = 'success', message = '', onclose }: { toastType: string; message: string; onclose: () => void } = $props()
</script>

<div
  role="alert"
  class="fixed top-4 right-4 z-50 px-4 py-3 rounded shadow-lg text-sm transition-all"
  class:bg-green-700={toastType === 'success'}
  class:bg-red-700={toastType === 'error'}
>
  <div class="flex items-center gap-2">
    <span class="text-white">{message}</span>
    <button onclick={onclose} class="text-white/70 hover:text-white text-lg leading-none">&times;</button>
  </div>
</div>
