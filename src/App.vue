<script setup lang="ts">
import { onMounted } from "vue";
import { RouterView } from "vue-router";
import AlertModal from "$lib/components/AlertModal.vue";
import { initAlertAudioStopListener } from "$lib/utils/alertSound";

onMounted(() => {
  void initAlertAudioStopListener();
});
</script>

<template>
  <RouterView />
  <AlertModal />
</template>
