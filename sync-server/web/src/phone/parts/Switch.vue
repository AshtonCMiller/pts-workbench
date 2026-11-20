<script setup>
// An on/off switch: <Switch v-model="on" />, or :model-value plus @update:model-value
defineProps({ modelValue: Boolean, title: { type: String, default: undefined } });
defineEmits(['update:modelValue']);
</script>

<template>
  <label class="switch" :title="title">
    <input type="checkbox" :checked="modelValue" @change="$emit('update:modelValue', $event.target.checked)"><i />
  </label>
</template>
